//! A read-only Model Context Protocol server at POST /api/mcp, so that an AI
//! assistant the user already has can look things up in their library.
//!
//! Transport: Streamable HTTP, one JSON-RPC message per POST, answered with a
//! single JSON object (no SSE, no sessions). Both protocol generations are
//! served on the same endpoint:
//!
//! - 2026-07-28, where every request carries its protocol version, mirrored
//!   in the `MCP-Protocol-Version`, `Mcp-Method` and `Mcp-Name` headers;
//! - 2025-03-26 to 2025-11-25, which open with an `initialize` handshake.
//!
//! Off unless LEGEJO_MCP=true. Authentication is an app password as a bearer
//! token; it also decides whose library is read. Every tool only reads.

use crate::auth::{AuthUser, UserInfo};
use crate::books::{self, Book};
use crate::booktext;
use crate::AppState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde_json::{json, Map, Value};

const MODERN: &str = "2026-07-28";
const LEGACY: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const HEADER_MISMATCH: i64 = -32020;
const UNSUPPORTED_VERSION: i64 = -32022;

const INSTRUCTIONS: &str = "Legejo is the user's personal EPUB library. Use these tools to look up their books, \
shelves and reading progress, and to read or search the text of a book. Everything is read-only. Positions are \
percentages of the book; when summarising for a reader who is partway through, do not reveal what comes after \
their reading position unless asked.";

/// How long a client may reuse the discovery and tool-list results.
const CACHE_TTL_MS: u64 = 3_600_000;

/// Text returned per call by read_section unless the caller asks for less.
const DEFAULT_CHARS: usize = 12_000;
const MAX_CHARS: usize = 30_000;

fn rpc_error(id: &Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn reply(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn server_info() -> Value {
    json!({ "name": "legejo", "title": "Legejo", "version": env!("CARGO_PKG_VERSION") })
}

/// The app password in `Authorization: Bearer …`, resolved to its user.
async fn authenticate(state: &AppState, headers: &HeaderMap) -> Option<UserInfo> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer "))?;
    crate::app_passwords::user_for(state, token.trim()).await
}

/// A browser page on another origin must not be able to drive the endpoint
/// (DNS rebinding); requests without an Origin header come from other clients.
fn origin_allowed(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let origin_host = origin.split("://").nth(1).unwrap_or("").to_ascii_lowercase();
    let request_host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    let public_host = state.fed.config.as_ref().map(|c| c.host.to_ascii_lowercase());
    !origin_host.is_empty() && (origin_host == request_host || Some(origin_host) == public_host)
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim)
}

/// `Mcp-Name` may carry "=?base64?…?=" for values that are not plain ASCII.
fn decode_header_value(value: &str) -> String {
    value
        .strip_prefix("=?base64?")
        .and_then(|v| v.strip_suffix("?="))
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| value.to_string())
}

pub async fn post(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if !state.settings.mcp {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !origin_allowed(&state, &headers) {
        return reply(StatusCode::FORBIDDEN, rpc_error(&Value::Null, INVALID_REQUEST, "origin not allowed", None));
    }
    let Some(user) = authenticate(&state, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"Legejo\"")],
            Json(rpc_error(&Value::Null, INVALID_REQUEST, "an app password is required as a bearer token", None)),
        )
            .into_response();
    };

    let message: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return reply(StatusCode::BAD_REQUEST, rpc_error(&Value::Null, PARSE_ERROR, "not JSON", None)),
    };
    let Some(object) = message.as_object() else {
        return reply(StatusCode::BAD_REQUEST, rpc_error(&Value::Null, INVALID_REQUEST, "one JSON-RPC message per request", None));
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        // A response from the client: nothing here ever asks for one.
        return StatusCode::ACCEPTED.into_response();
    };
    let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
    let Some(id) = object.get("id").filter(|id| !id.is_null()).cloned() else {
        // Notifications (notifications/initialized, cancellations) need no answer.
        return StatusCode::ACCEPTED.into_response();
    };

    let version_header = header_str(&headers, "mcp-protocol-version");
    let meta_version = params.pointer("/_meta/io.modelcontextprotocol~1protocolVersion").and_then(Value::as_str);
    let legacy = method == "initialize" || (meta_version.is_none() && version_header.is_none_or(|v| LEGACY.contains(&v)));
    if legacy {
        return legacy_request(&state, &user, &id, method, &params).await;
    }
    modern_request(&state, &user, &headers, &id, method, &params, version_header, meta_version).await
}

/// Protocol versions with an `initialize` handshake. Nothing is remembered
/// between requests, so no session id is issued.
async fn legacy_request(state: &AppState, user: &UserInfo, id: &Value, method: &str, params: &Value) -> Response {
    let result = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = if LEGACY.contains(&requested) { requested } else { LEGACY[0] };
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": server_info(),
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_definitions() }),
        "tools/call" => match call_tool(state, user, params).await {
            Ok(result) => result,
            Err(message) => return reply(StatusCode::OK, rpc_error(id, INVALID_PARAMS, &message, None)),
        },
        _ => return reply(StatusCode::OK, rpc_error(id, METHOD_NOT_FOUND, "method not found", None)),
    };
    reply(StatusCode::OK, json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

#[allow(clippy::too_many_arguments)]
async fn modern_request(
    state: &AppState,
    user: &UserInfo,
    headers: &HeaderMap,
    id: &Value,
    method: &str,
    params: &Value,
    version_header: Option<&str>,
    meta_version: Option<&str>,
) -> Response {
    let mismatch = |message: &str| reply(StatusCode::BAD_REQUEST, rpc_error(id, HEADER_MISMATCH, &format!("Header mismatch: {message}"), None));
    let Some(version) = version_header else {
        return mismatch("the MCP-Protocol-Version header is missing");
    };
    if meta_version.is_some_and(|m| m != version) {
        return mismatch("MCP-Protocol-Version does not match the protocol version in _meta");
    }
    if version != MODERN {
        let mut supported = vec![MODERN];
        supported.extend(LEGACY);
        return reply(
            StatusCode::BAD_REQUEST,
            rpc_error(id, UNSUPPORTED_VERSION, "Unsupported protocol version", Some(json!({ "supported": supported, "requested": version }))),
        );
    }
    if header_str(headers, "mcp-method") != Some(method) {
        return mismatch("the Mcp-Method header is missing or does not match the method");
    }
    if method == "tools/call" {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        if header_str(headers, "mcp-name").map(decode_header_value).as_deref() != Some(name) {
            return mismatch("the Mcp-Name header is missing or does not match the tool name");
        }
    }

    // The discovery and tool-list results are the same for every user and
    // only change with a new release, so clients may cache them.
    let cacheable = |mut result: Value| {
        result["ttlMs"] = json!(CACHE_TTL_MS);
        result["cacheScope"] = json!("public");
        result
    };
    let mut result = match method {
        "server/discover" => cacheable(json!({
            "supportedVersions": [MODERN],
            "capabilities": { "tools": {} },
            "instructions": INSTRUCTIONS,
        })),
        "tools/list" => cacheable(json!({ "tools": tool_definitions() })),
        "tools/call" => match call_tool(state, user, params).await {
            Ok(result) => result,
            Err(message) => return reply(StatusCode::BAD_REQUEST, rpc_error(id, INVALID_PARAMS, &message, None)),
        },
        _ => return reply(StatusCode::NOT_FOUND, rpc_error(id, METHOD_NOT_FOUND, "Method not found", None)),
    };
    result["resultType"] = json!("complete");
    result["_meta"] = json!({ "io.modelcontextprotocol/serverInfo": server_info() });
    reply(StatusCode::OK, json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

// ---- Tools ------------------------------------------------------------------

fn tool(name: &str, title: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
        "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
    })
}

fn tool_definitions() -> Value {
    let book_id = json!({ "type": "integer", "description": "The book's id, from search_books." });
    json!([
        tool(
            "search_books",
            "Search the library",
            "List books in the user's library, newest first, or search them by title, author, description and category. \
             Can be narrowed to a shelf or a reading status.",
            json!({
                "query": { "type": "string", "description": "Words to search for. Leave out to list everything." },
                "shelf_id": { "type": "integer", "description": "Only books on this shelf (see list_shelves)." },
                "status": { "type": "string", "enum": ["reading", "finished", "unread", "want_to_read"], "description": "Only books with this reading status." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Most books to return. Default 25." },
            }),
            &[],
        ),
        tool(
            "get_book",
            "Book details",
            "Everything known about one book: description, tags, shelves, series, rating and reading progress.",
            json!({ "book_id": book_id }),
            &["book_id"],
        ),
        tool("list_shelves", "List shelves", "The user's shelves, with how many books each holds.", json!({}), &[]),
        tool(
            "reading_overview",
            "Reading overview",
            "What the user is reading now, wants to read and finished most recently, with totals for the library.",
            json!({}),
            &[],
        ),
        tool(
            "get_table_of_contents",
            "Table of contents",
            "The sections of a book in reading order, with their length and where each starts and ends as a percentage \
             of the book, and the user's reading position. Call this before read_section.",
            json!({ "book_id": book_id }),
            &["book_id"],
        ),
        tool(
            "read_section",
            "Read a section",
            "The text of one section of a book. Long sections come in pieces: pass the returned next_offset to continue.",
            json!({
                "book_id": book_id,
                "section": { "type": "integer", "minimum": 0, "description": "The section's index, from get_table_of_contents." },
                "offset": { "type": "integer", "minimum": 0, "description": "Where to start within the section. Default 0." },
                "max_chars": { "type": "integer", "minimum": 500, "maximum": MAX_CHARS, "description": "Most text to return. Default 12000." },
            }),
            &["book_id", "section"],
        ),
        tool(
            "search_in_book",
            "Search inside a book",
            "Find a word or phrase in the text of a book. Returns each match with its section, position and the text around it.",
            json!({
                "book_id": book_id,
                "query": { "type": "string", "description": "The word or phrase to find; case is ignored." },
                "limit": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Most matches to return. Default 20." },
            }),
            &["book_id", "query"],
        ),
    ])
}

/// Err is a protocol error (unknown tool, bad arguments); a tool that runs
/// but fails answers with `isError`, which the model can read and act on.
async fn call_tool(state: &AppState, user: &UserInfo, params: &Value) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("tools/call needs a tool name")?;
    let empty = Map::new();
    let args = params.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
    let outcome = match name {
        "search_books" => search_books(state, user, args).await,
        "get_book" => get_book(state, user, args).await,
        "list_shelves" => list_shelves(state, user).await,
        "reading_overview" => reading_overview(state, user).await,
        "get_table_of_contents" => table_of_contents(state, user, args).await,
        "read_section" => read_section(state, user, args).await,
        "search_in_book" => search_in_book(state, user, args).await,
        _ => return Err(format!("unknown tool: {name}")),
    };
    Ok(match outcome {
        Ok(data) => json!({
            "content": [{ "type": "text", "text": data.to_string() }],
            "structuredContent": data,
            "isError": false,
        }),
        Err(message) => json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
    })
}

type ToolResult = Result<Value, String>;

fn as_user(user: &UserInfo) -> AuthUser {
    AuthUser(UserInfo {
        id: user.id,
        username: user.username.clone(),
        is_admin: user.is_admin,
        locale: user.locale.clone(),
        has_avatar: user.has_avatar,
    })
}

/// What a handler's error response means to the model.
fn handler_error(response: Response) -> String {
    match response.status() {
        StatusCode::NOT_FOUND => "not found in this library".into(),
        status => format!("the library could not answer ({status})"),
    }
}

fn int_arg(args: &Map<String, Value>, name: &str) -> Option<i64> {
    args.get(name).and_then(Value::as_i64)
}

fn required_int(args: &Map<String, Value>, name: &str) -> Result<i64, String> {
    int_arg(args, name).ok_or_else(|| format!("{name} is required and must be an integer"))
}

fn status_of(book: &Book) -> &'static str {
    match book.progress_percent {
        None => "unread",
        Some(p) if p >= 0.99 => "finished",
        Some(_) => "reading",
    }
}

fn percent(fraction: f64) -> f64 {
    (fraction * 1000.0).round() / 10.0
}

fn book_summary(book: &Book, shelf_ids: Option<&[i64]>) -> Value {
    let mut v = json!({
        "id": book.id,
        "title": book.title,
        "author": book.author,
        "status": status_of(book),
    });
    let mut set = |key: &str, value: Value| {
        if !value.is_null() {
            v[key] = value;
        }
    };
    set("series", json!(book.series));
    set("series_index", json!(book.series_index));
    set("first_published", json!(book.first_published));
    set("language", json!(book.language));
    set("rating", json!(book.rating));
    set("progress_percent", json!(book.progress_percent.map(percent)));
    set("last_read_at", json!(book.last_read_at));
    if book.want_to_read.as_bool() {
        set("want_to_read", json!(true));
    }
    if let Some(ids) = shelf_ids.filter(|ids| !ids.is_empty()) {
        set("shelf_ids", json!(ids));
    }
    v
}

async fn library(state: &AppState, user: &UserInfo, query: Option<String>) -> Result<Vec<books::BookListItem>, String> {
    books::list(State(state.clone()), as_user(user), Query(books::ListParams::new(query)))
        .await
        .map(|Json(items)| items)
        .map_err(handler_error)
}

async fn search_books(state: &AppState, user: &UserInfo, args: &Map<String, Value>) -> ToolResult {
    let query = args.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()).map(str::to_string);
    let shelf = int_arg(args, "shelf_id");
    let status = args.get("status").and_then(Value::as_str);
    let limit = int_arg(args, "limit").unwrap_or(25).clamp(1, 100) as usize;
    let matching: Vec<books::BookListItem> = library(state, user, query)
        .await?
        .into_iter()
        .filter(|item| shelf.is_none_or(|s| item.shelf_ids.contains(&s)))
        .filter(|item| match status {
            None => true,
            Some("want_to_read") => item.book.want_to_read.as_bool(),
            Some(wanted) => status_of(&item.book) == wanted,
        })
        .collect();
    let books: Vec<Value> = matching.iter().take(limit).map(|item| book_summary(&item.book, Some(&item.shelf_ids))).collect();
    Ok(json!({ "total": matching.len(), "returned": books.len(), "books": books }))
}

async fn get_book(state: &AppState, user: &UserInfo, args: &Map<String, Value>) -> ToolResult {
    let id = required_int(args, "book_id")?;
    let Json(detail) = books::get_one(State(state.clone()), as_user(user), Path(id)).await.map_err(handler_error)?;
    let mut v = book_summary(&detail.book, None);
    let book = &detail.book;
    for (key, value) in [
        ("description", json!(book.description)),
        ("publisher", json!(book.publisher)),
        ("edition_date", json!(book.published)),
        ("category", json!(book.category)),
        ("isbn", json!(book.isbn)),
        ("license", json!(book.license)),
        ("added_at", json!(book.created_at)),
    ] {
        if !value.is_null() {
            v[key] = value;
        }
    }
    v["tags"] = json!(detail.tags);
    v["shelves"] = json!(detail.shelves);
    Ok(v)
}

async fn list_shelves(state: &AppState, user: &UserInfo) -> ToolResult {
    let Json(shelves) = crate::shelves::list(State(state.clone()), as_user(user)).await.map_err(handler_error)?;
    let shelves: Vec<Value> = shelves
        .iter()
        .map(|s| json!({ "id": s.id, "name": s.name, "books": s.book_count, "visibility": s.visibility, "description": s.description }))
        .collect();
    Ok(json!({ "shelves": shelves }))
}

async fn reading_overview(state: &AppState, user: &UserInfo) -> ToolResult {
    let items = library(state, user, None).await?;
    let all: Vec<&Book> = items.iter().map(|i| &i.book).collect();
    fn by_last_read(mut books: Vec<&Book>) -> Vec<&Book> {
        books.sort_by(|a, b| b.last_read_at.cmp(&a.last_read_at));
        books
    }
    let reading = by_last_read(all.iter().copied().filter(|b| status_of(b) == "reading").collect());
    let finished = by_last_read(all.iter().copied().filter(|b| status_of(b) == "finished").collect());
    let mut wanted: Vec<&Book> = all.iter().copied().filter(|b| b.want_to_read.as_bool()).collect();
    wanted.sort_by(|a, b| b.wanted_at.cmp(&a.wanted_at));
    let list = |books: &[&Book], n: usize| books.iter().take(n).map(|b| book_summary(b, None)).collect::<Vec<_>>();
    Ok(json!({
        "totals": {
            "books": all.len(),
            "reading": reading.len(),
            "finished": finished.len(),
            "want_to_read": wanted.len(),
            "unread": all.len() - reading.len() - finished.len(),
        },
        "reading_now": list(&reading, 20),
        "want_to_read": list(&wanted, 30),
        "recently_finished": list(&finished, 10),
    }))
}

/// The book's text, with its title and the user's reading position (0–1).
async fn book_text(state: &AppState, user: &UserInfo, id: i64) -> Result<(String, Option<f64>, Vec<booktext::Section>), String> {
    let Json(detail) = books::get_one(State(state.clone()), as_user(user), Path(id)).await.map_err(handler_error)?;
    let path = state.data_dir.join("books").join(format!("{}.epub", detail.book.uuid));
    let bytes = tokio::fs::read(&path).await.map_err(|_| "the book's file is missing".to_string())?;
    let sections = tokio::task::spawn_blocking(move || booktext::sections(bytes))
        .await
        .map_err(|_| "the book could not be read".to_string())?
        .map_err(|e| format!("the book could not be read: {e}"))?;
    Ok((detail.book.title, detail.book.progress_percent, sections))
}

fn section_title(section: &booktext::Section, index: usize) -> String {
    section.title.clone().unwrap_or_else(|| format!("Section {}", index + 1))
}

async fn table_of_contents(state: &AppState, user: &UserInfo, args: &Map<String, Value>) -> ToolResult {
    let (title, position, sections) = book_text(state, user, required_int(args, "book_id")?).await?;
    let list: Vec<Value> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "section": i,
                "title": section_title(s, i),
                "chars": s.text.chars().count(),
                "starts_at_percent": percent(s.start),
                "ends_at_percent": percent(s.end),
            })
        })
        .collect();
    let mut v = json!({ "title": title, "sections": list });
    if let Some(p) = position {
        v["reading_position_percent"] = json!(percent(p));
        v["reading_position_section"] = json!(sections.iter().position(|s| p < s.end).unwrap_or(sections.len().saturating_sub(1)));
    }
    Ok(v)
}

async fn read_section(state: &AppState, user: &UserInfo, args: &Map<String, Value>) -> ToolResult {
    let (_, _, sections) = book_text(state, user, required_int(args, "book_id")?).await?;
    let index = required_int(args, "section")?;
    let section = usize::try_from(index).ok().and_then(|i| sections.get(i)).ok_or_else(|| format!("the book has sections 0 to {}", sections.len().saturating_sub(1)))?;
    let max_chars = int_arg(args, "max_chars").map_or(DEFAULT_CHARS, |n| n.clamp(500, MAX_CHARS as i64) as usize);
    let start = booktext::floor_boundary(&section.text, int_arg(args, "offset").unwrap_or(0).max(0) as usize);
    let rest = &section.text[start..];
    // Up to max_chars characters, ending at a line break when one is near.
    let mut end = rest.char_indices().nth(max_chars).map_or(rest.len(), |(i, _)| i);
    if end < rest.len() {
        if let Some(line_break) = rest[..end].rfind('\n').filter(|i| *i > end / 2) {
            end = line_break;
        }
    }
    let next = start + end;
    let span = section.end - section.start;
    let at = |offset: usize| percent(section.start + span * offset as f64 / section.text.len().max(1) as f64);
    Ok(json!({
        "section": index,
        "title": section_title(section, index as usize),
        "text": &rest[..end],
        "from_percent": at(start),
        "to_percent": at(next),
        "next_offset": if next < section.text.len() { json!(next) } else { Value::Null },
    }))
}

async fn search_in_book(state: &AppState, user: &UserInfo, args: &Map<String, Value>) -> ToolResult {
    let (_, _, sections) = book_text(state, user, required_int(args, "book_id")?).await?;
    let query = args.get("query").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()).ok_or("query is required")?;
    let limit = int_arg(args, "limit").unwrap_or(20).clamp(1, 50) as usize;
    let mut matches = Vec::new();
    let mut total = 0usize;
    for (i, section) in sections.iter().enumerate() {
        let found = booktext::find_all(&section.text, query, 10_000);
        total += found.len();
        for at in found {
            if matches.len() >= limit {
                break;
            }
            let from = booktext::floor_boundary(&section.text, at.saturating_sub(160));
            let to = booktext::floor_boundary(&section.text, at + query.len() + 160);
            let span = section.end - section.start;
            matches.push(json!({
                "section": i,
                "title": section_title(section, i),
                "offset": at,
                "at_percent": percent(section.start + span * at as f64 / section.text.len().max(1) as f64),
                "context": section.text[from..to].replace('\n', " "),
            }));
        }
    }
    Ok(json!({ "total": total, "returned": matches.len(), "matches": matches }))
}
