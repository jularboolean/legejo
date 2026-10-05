//! Metadata and cover lookup against Open Library (openlibrary.org), as a
//! complement to Libris. Disabled by default; enabled by an admin setting.
//!
//! To keep load on the free service low:
//! - responses are stored in `metadata_cache`, shared by all users, for 30
//!   days (1 day for empty responses), so each book is looked up at most once
//!   per instance and the cache survives restarts;
//! - covers are cached under data/cache/openlibrary and proxied, so browsers
//!   never contact covers.openlibrary.org directly;
//! - API requests are limited to one per second (THROTTLE; cover images by
//!   id, which are not rate limited upstream, to four), and every request
//!   carries a User-Agent with the instance and a contact address, as Open
//!   Library requests.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const API: &str = "https://openlibrary.org";
const COVERS: &str = "https://covers.openlibrary.org";
const THROTTLE: std::time::Duration = std::time::Duration::from_millis(1000);
/// Open Library rate limits covers by ISBN and similar keys, not by cover id.
const COVER_THROTTLE: std::time::Duration = std::time::Duration::from_millis(250);
const HIT_DAYS: i64 = 30;
const MISS_DAYS: i64 = 1;
const COVER_LIMIT: usize = 10 * 1024 * 1024;

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn upstream(e: impl std::fmt::Display) -> Response {
    tracing::warn!("open library: {e}");
    err(StatusCode::BAD_GATEWAY, "could not reach Open Library")
}

pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "openlibrary_enabled", false).await.unwrap_or(false)
}

async fn ensure_enabled(state: &AppState) -> Result<(), Response> {
    if enabled(state).await {
        Ok(())
    } else {
        Err(err(StatusCode::FORBIDDEN, "Open Library lookup is disabled"))
    }
}

type Gate = std::sync::OnceLock<tokio::sync::Mutex<Option<tokio::time::Instant>>>;

/// One request at a time through `gate`, at least `gap` apart, process-wide.
async fn wait_turn(gate: &'static Gate, gap: std::time::Duration) {
    let mut last = gate.get_or_init(|| tokio::sync::Mutex::new(None)).lock().await;
    if let Some(at) = *last {
        let since = at.elapsed();
        if since < gap {
            tokio::time::sleep(gap - since).await;
        }
    }
    *last = Some(tokio::time::Instant::now());
}

async fn throttle() {
    static API_GATE: Gate = std::sync::OnceLock::new();
    wait_turn(&API_GATE, THROTTLE).await;
}

async fn throttle_covers() {
    static COVER_GATE: Gate = std::sync::OnceLock::new();
    wait_turn(&COVER_GATE, COVER_THROTTLE).await;
}

fn user_agent(state: &AppState) -> String {
    let site = state.fed.config.as_ref().map(|c| c.base.clone()).unwrap_or_else(|| "self-hosted".into());
    match &state.mail {
        Some(m) => format!("Legejo/{} ({site}; {})", env!("CARGO_PKG_VERSION"), m.from),
        None => format!("Legejo/{} ({site})", env!("CARGO_PKG_VERSION")),
    }
}

fn days_ago(days: i64) -> String {
    let t = time::OffsetDateTime::now_utc() - time::Duration::days(days);
    t.format(time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"))
        .unwrap_or_default()
}

/// A cached JSON answer, if fresh: non-empty for HIT_DAYS, empty (null) for MISS_DAYS.
async fn cached(state: &AppState, key: &str) -> Option<Value> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT body, fetched_at FROM metadata_cache WHERE key = $1")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let (body, at) = row?;
    let value: Value = serde_json::from_str(&body).ok()?;
    let ttl = if value.is_null() { MISS_DAYS } else { HIT_DAYS };
    (at > days_ago(ttl)).then_some(value)
}

async fn remember(state: &AppState, key: &str, value: &Value) {
    let _ = sqlx::query(
        "INSERT INTO metadata_cache (key, body, fetched_at) VALUES ($1, $2, $3)
         ON CONFLICT (key) DO UPDATE SET body = excluded.body, fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(value.to_string())
    .bind(now_ts())
    .execute(&state.db)
    .await;
}

/// GET a JSON document from Open Library through cache and throttle.
/// 404 is remembered as null (a short-lived miss).
async fn fetch_json(state: &AppState, key: &str, url: &str) -> Result<(Value, bool), Response> {
    if let Some(v) = cached(state, key).await {
        return Ok((v, true));
    }
    throttle().await;
    let res = state
        .http
        .get(url)
        .header(header::USER_AGENT, user_agent(state))
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(upstream)?;
    let value = match res.status() {
        s if s.is_success() => res.json::<Value>().await.map_err(upstream)?,
        StatusCode::NOT_FOUND => Value::Null,
        s => return Err(upstream(format!("HTTP {s} for {url}"))),
    };
    remember(state, key, &value).await;
    Ok((value, false))
}

#[derive(Deserialize)]
pub struct SearchParams {
    isbn: Option<String>,
    title: Option<String>,
    author: Option<String>,
}

#[derive(Serialize)]
pub struct Candidate {
    /// The work, e.g. "OL45804W" (for the description).
    work: Option<String>,
    title: Option<String>,
    author: Option<String>,
    publisher: Option<String>,
    year: Option<i64>,
    isbn: Vec<String>,
    /// ISO 639-2 ("swe"), as Open Library gives it.
    language: Option<String>,
    cover_id: Option<i64>,
}

#[derive(Serialize)]
pub struct SearchResult {
    found: i64,
    candidates: Vec<Candidate>,
    /// Served from the cache without contacting Open Library.
    cached: bool,
}

fn clean_isbn(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x').collect::<String>().to_uppercase()
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

const FIELDS: &str = "key,title,author_name,first_publish_year,publisher,isbn,language,cover_i";

pub async fn search(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<SearchParams>,
) -> Result<Json<SearchResult>, Response> {
    ensure_enabled(&state).await?;
    let isbn = q.isbn.as_deref().map(clean_isbn).filter(|i| i.len() == 10 || i.len() == 13);
    let title = q.title.as_deref().map(norm).filter(|t| !t.is_empty());
    let author = q.author.as_deref().map(norm).filter(|a| !a.is_empty());
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let (key, url) = if let Some(isbn) = &isbn {
        (format!("ol:search:isbn:{isbn}"), format!("{API}/search.json?isbn={isbn}&fields={FIELDS}&limit=8"))
    } else if let Some(title) = &title {
        let mut url = format!("{API}/search.json?title={}&fields={FIELDS}&limit=8", enc(title));
        if let Some(a) = &author {
            url.push_str(&format!("&author={}", enc(a)));
        }
        (format!("ol:search:t:{title}|a:{}", author.as_deref().unwrap_or("")), url)
    } else {
        return Err(err(StatusCode::BAD_REQUEST, "give an ISBN or a title"));
    };
    let (value, cached) = fetch_json(&state, &key, &url).await?;
    let docs = value.get("docs").and_then(Value::as_array).cloned().unwrap_or_default();
    let s = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).map(str::to_string);
    let first = |d: &Value, k: &str| d.get(k).and_then(Value::as_array).and_then(|a| a.first()).and_then(Value::as_str).map(str::to_string);
    let candidates = docs
        .iter()
        .map(|d| Candidate {
            work: s(d, "key").map(|k| k.trim_start_matches("/works/").to_string()),
            title: s(d, "title"),
            author: d.get("author_name").and_then(Value::as_array).map(|a| {
                a.iter().filter_map(Value::as_str).take(3).collect::<Vec<_>>().join(", ")
            }),
            publisher: first(d, "publisher"),
            year: d.get("first_publish_year").and_then(Value::as_i64),
            isbn: d
                .get("isbn")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).filter(|i| i.len() == 13).take(3).map(str::to_string).collect())
                .unwrap_or_default(),
            language: first(d, "language"),
            cover_id: d.get("cover_i").and_then(Value::as_i64),
        })
        .collect();
    Ok(Json(SearchResult { found: value.get("numFound").and_then(Value::as_i64).unwrap_or(0), candidates, cached }))
}

#[derive(Deserialize)]
pub struct WorkParams {
    key: String,
}

/// The work's description and subjects (for an empty description field).
pub async fn work(State(state): State<AppState>, _user: AuthUser, Query(q): Query<WorkParams>) -> Result<Json<Value>, Response> {
    ensure_enabled(&state).await?;
    let key = q.key.trim().trim_start_matches("/works/");
    if !key.starts_with("OL") || !key.ends_with('W') || !key[2..key.len() - 1].chars().all(|c| c.is_ascii_digit()) {
        return Err(err(StatusCode::BAD_REQUEST, "not a work key"));
    }
    let (value, _) = fetch_json(&state, &format!("ol:work:{key}"), &format!("{API}/works/{key}.json")).await?;
    let description = match value.get("description") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(o)) => o.get("value").and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    let subjects: Vec<String> = value
        .get("subjects")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).take(10).map(str::to_string).collect())
        .unwrap_or_default();
    Ok(Json(json!({ "description": description, "subjects": subjects })))
}

fn cover_path(state: &AppState, id: i64, size: char) -> std::path::PathBuf {
    state.data_dir.join("cache").join("openlibrary").join(format!("{id}-{size}.jpg"))
}

/// The cover image, from the file cache or Open Library (throttled).
async fn cover_bytes(state: &AppState, id: i64, size: char) -> Result<Vec<u8>, Response> {
    let path = cover_path(state, id, size);
    if let Ok(meta) = tokio::fs::metadata(&path).await {
        let fresh = meta.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age.as_secs() < (HIT_DAYS * 86400) as u64);
        if fresh {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Ok(bytes);
            }
        }
    }
    throttle_covers().await;
    // default=false: a 404 instead of a blank placeholder image.
    let url = format!("{COVERS}/b/id/{id}-{size}.jpg?default=false");
    let res = state.http.get(&url).header(header::USER_AGENT, user_agent(state)).send().await.map_err(upstream)?;
    if res.status() == StatusCode::NOT_FOUND {
        return Err(err(StatusCode::NOT_FOUND, "no cover"));
    }
    if !res.status().is_success() {
        return Err(upstream(format!("HTTP {} for {url}", res.status())));
    }
    let bytes = res.bytes().await.map_err(upstream)?;
    if bytes.len() > COVER_LIMIT || !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(upstream("cover is not a JPEG"));
    }
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let tmp = path.with_extension(format!("{}.tmp", crate::books::new_uuid()));
    if tokio::fs::write(&tmp, &bytes).await.is_ok() {
        let _ = tokio::fs::rename(&tmp, &path).await;
    }
    Ok(bytes.to_vec())
}

#[derive(Deserialize)]
pub struct CoverParams {
    size: Option<String>,
}

/// GET /api/openlibrary/cover/{id}?size=S|M|L: cover previews for the edit page.
pub async fn cover(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    Query(q): Query<CoverParams>,
) -> Result<Response, Response> {
    ensure_enabled(&state).await?;
    let size = match q.size.as_deref() {
        Some("S") => 'S',
        Some("L") => 'L',
        _ => 'M',
    };
    let bytes = cover_bytes(&state, id, size).await?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=86400")], bytes).into_response())
}

#[derive(Deserialize)]
pub struct UseCover {
    cover_id: i64,
}

/// POST /api/books/{id}/cover/openlibrary: set an Open Library cover as the book's cover.
pub async fn use_cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UseCover>,
) -> Result<Json<crate::books::CoverResult>, Response> {
    ensure_enabled(&state).await?;
    let uuid: Option<String> = sqlx::query_scalar("SELECT uuid FROM books WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("internal error: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        })?;
    let uuid = uuid.ok_or_else(|| err(StatusCode::NOT_FOUND, "not found"))?;
    let bytes = cover_bytes(&state, req.cover_id, 'L').await?;
    crate::books::apply_cover(&state, &user, id, &uuid, bytes, "image/jpeg".into()).await
}

/// Run daily: delete expired cache rows and cover files.
pub async fn prune(state: &AppState) {
    let _ = sqlx::query("DELETE FROM metadata_cache WHERE fetched_at < $1").bind(days_ago(HIT_DAYS * 2)).execute(&state.db).await;
    let dir = state.data_dir.join("cache").join("openlibrary");
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else { return };
    while let Ok(Some(e)) = entries.next_entry().await {
        let old = e.metadata().await.ok().and_then(|m| m.modified().ok()).and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age.as_secs() > (HIT_DAYS * 2 * 86400) as u64);
        if old {
            let _ = tokio::fs::remove_file(e.path()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    #[tokio::test]
    async fn off_by_default_and_answers_from_the_cache() {
        let (app, db, _) = test_app().await;
        let (_, cookie) = add_user(&db, "alice").await;
        let (status, _) = send(&app, Method::GET, "/api/openlibrary/search?isbn=9780141439518", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        sqlx::query("INSERT INTO settings (key, value) VALUES ('openlibrary_enabled', 'true')").execute(&db).await.unwrap();
        // A cached answer is used as is: no request leaves the test.
        let body = json!({ "numFound": 1, "docs": [{
            "key": "/works/OL66554W", "title": "Pride and Prejudice", "author_name": ["Jane Austen"],
            "first_publish_year": 1813, "publisher": ["Penguin"], "isbn": ["0141439513", "9780141439518"],
            "language": ["eng"], "cover_i": 12645114
        }]});
        sqlx::query("INSERT INTO metadata_cache (key, body, fetched_at) VALUES ('ol:search:isbn:9780141439518', ?, ?)")
            .bind(body.to_string())
            .bind(crate::db::now_ts())
            .execute(&db)
            .await
            .unwrap();
        let (status, v) = send(&app, Method::GET, "/api/openlibrary/search?isbn=978-0-14-143951-8", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["cached"], true);
        let c = &v["candidates"][0];
        assert_eq!(c["work"], "OL66554W");
        assert_eq!(c["author"], "Jane Austen");
        assert_eq!(c["year"], 1813);
        assert_eq!(c["isbn"], json!(["9780141439518"]));
        assert_eq!(c["cover_id"], 12645114);

        let (status, _) = send(&app, Method::GET, "/api/openlibrary/work?key=../../etc", Some(&cookie), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
