//! The MCP endpoint: authentication, both protocol generations and the tools.

use crate::library_tests::upload;
use crate::settings::Settings;
use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::AnyPool;
use std::sync::Arc;
use tower::ServiceExt;

/// An EPUB with a cover page without text and two chapters named in the NCX.
fn novel(title: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut file = |name: &str, content: String| {
        z.start_file(name, stored).unwrap();
        z.write_all(content.as_bytes()).unwrap();
    };
    file("mimetype", "application/epub+zip".into());
    file(
        "META-INF/container.xml",
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into(),
    );
    file(
        "c.opf",
        format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:creator>Selma Lagerlöf</dc:creator><dc:language>sv</dc:language><dc:identifier id="i">urn:test:{title}</dc:identifier></metadata><manifest><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/><item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="one.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine toc="ncx"><itemref idref="cover"/><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#
        ),
    );
    file(
        "toc.ncx",
        r#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head/><docTitle><text>t</text></docTitle><navMap><navPoint id="n1" playOrder="1"><navLabel><text>The Storm</text></navLabel><content src="one.xhtml"/></navPoint><navPoint id="n2" playOrder="2"><navLabel><text>The Harbour</text></navLabel><content src="two.xhtml#start"/></navPoint></navMap></ncx>"#.into(),
    );
    let page = |body: String| format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>x</title></head><body>{body}</body></html>"#);
    file("cover.xhtml", page(r#"<img src="cover.jpg"/>"#.into()));
    let storm: String = (1..=60).map(|i| format!("<p>Paragraph {i} of the storm at sea.</p>")).collect();
    file("one.xhtml", page(format!("<h1>The Storm</h1>{storm}")));
    file("two.xhtml", page("<h1 id=\"start\">The Harbour</h1><p>The lighthouse keeper saw the ship at dawn.</p><p>Nobody spoke of the Lighthouse again.</p>".into()));
    z.finish().unwrap().into_inner()
}

struct Client {
    app: Router,
    token: String,
}

impl Client {
    /// POST one JSON-RPC message; returns the HTTP status and the body.
    async fn post(&self, headers: &[(&str, &str)], body: Value) -> (StatusCode, Value) {
        let mut req = Request::builder().method(Method::POST).uri("/api/mcp").header(header::CONTENT_TYPE, "application/json");
        if !self.token.is_empty() {
            req = req.header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        }
        for (name, value) in headers {
            req = req.header(*name, *value);
        }
        let res = self.app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// A tool call the way a 2025-era client makes it; returns the structured result.
    async fn call(&self, name: &str, arguments: Value) -> Value {
        let (status, v) = self
            .post(&[("mcp-protocol-version", "2025-06-18")], json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": arguments } }))
            .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["result"]["isError"], false, "{v}");
        // The text content is the same data, for clients without structured results.
        let text: Value = serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, v["result"]["structuredContent"]);
        v["result"]["structuredContent"].clone()
    }

    async fn call_error(&self, name: &str, arguments: Value) -> String {
        let (_, v) = self
            .post(&[], json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": arguments } }))
            .await;
        assert_eq!(v["result"]["isError"], true, "{v}");
        v["result"]["content"][0]["text"].as_str().unwrap().to_string()
    }
}

/// An app with the endpoint turned on, user alice, an app password for her
/// and a client using it.
async fn setup() -> (Client, AnyPool, String) {
    let (_, db, mut state) = test_app().await;
    let mut settings = Settings::default();
    settings.mcp = true;
    state.settings = Arc::new(settings);
    let app = crate::router(state);
    let (_, cookie) = add_user(&db, "alice").await;
    let (status, v) = send(&app, Method::POST, "/api/account/app-passwords", Some(&cookie), Some(json!({ "name": "Assistant" }))).await;
    assert_eq!(status, StatusCode::CREATED);
    (Client { app, token: v["secret"].as_str().unwrap().to_string() }, db, cookie)
}

#[tokio::test]
async fn needs_an_app_password_and_a_same_origin() {
    let (client, _, _) = setup().await;
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });

    let anonymous = Client { app: client.app.clone(), token: String::new() };
    let req = Request::builder().method(Method::POST).uri("/api/mcp").body(Body::from(ping.to_string())).unwrap();
    let res = anonymous.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(res.headers().get(header::WWW_AUTHENTICATE).unwrap().to_str().unwrap().starts_with("Bearer"));
    let wrong = Client { app: client.app.clone(), token: "abcd-efgh-jkmn-pqrs".into() };
    assert_eq!(wrong.post(&[], ping.clone()).await.0, StatusCode::UNAUTHORIZED);

    assert_eq!(client.post(&[], ping.clone()).await, (StatusCode::OK, json!({ "jsonrpc": "2.0", "id": 1, "result": {} })));
    // Clients that keep Authorization for OAuth send the app password as an API key.
    assert_eq!(anonymous.post(&[("x-api-key", &client.token)], ping.clone()).await.0, StatusCode::OK);
    assert_eq!(anonymous.post(&[("x-api-key", "abcd-efgh-jkmn-pqrs")], ping.clone()).await.0, StatusCode::UNAUTHORIZED);
    // A page on another origin is refused; the instance's own origin is not.
    assert_eq!(client.post(&[("origin", "https://evil.example"), ("host", "books.example")], ping.clone()).await.0, StatusCode::FORBIDDEN);
    assert_eq!(client.post(&[("origin", "https://books.example"), ("host", "books.example")], ping.clone()).await.0, StatusCode::OK);

    // Only POST; no SSE stream and no sessions to end.
    for method in [Method::GET, Method::DELETE] {
        let req = Request::builder().method(method).uri("/api/mcp").body(Body::empty()).unwrap();
        assert_eq!(client.app.clone().oneshot(req).await.unwrap().status(), StatusCode::METHOD_NOT_ALLOWED);
    }
    assert_eq!(client.post(&[], json!([ping])).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn is_off_unless_turned_on() {
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let (_, v) = send(&app, Method::POST, "/api/account/app-passwords", Some(&cookie), Some(json!({ "name": "x" }))).await;
    let client = Client { app, token: v["secret"].as_str().unwrap().to_string() };
    assert_eq!(client.post(&[], json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" })).await.0, StatusCode::NOT_FOUND);
    // The web hides its MCP section accordingly.
    assert_eq!(send(&client.app, Method::GET, "/api/config", Some(&cookie), None).await.1["mcp_enabled"], false);
    let (on, _, on_cookie) = setup().await;
    assert_eq!(send(&on.app, Method::GET, "/api/config", Some(&on_cookie), None).await.1["mcp_enabled"], true);
}

#[tokio::test]
async fn speaks_the_handshake_generation() {
    let (client, _, _) = setup().await;
    let init = |version: &str| json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } });

    let (status, v) = client.post(&[], init("2025-06-18")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(v["result"]["serverInfo"]["name"], "legejo");
    assert!(v["result"]["capabilities"]["tools"].is_object());
    assert!(v["result"]["instructions"].as_str().unwrap().contains("read-only"));
    // An unknown version is answered with the newest one this generation has.
    assert_eq!(client.post(&[], init("2024-01-01")).await.1["result"]["protocolVersion"], "2025-11-25");

    let (status, body) = client.post(&[], json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!((status, body), (StatusCode::ACCEPTED, Value::Null));

    let (_, v) = client.post(&[("mcp-protocol-version", "2025-11-25")], json!({ "jsonrpc": "2.0", "id": "a", "method": "tools/list" })).await;
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["search_books", "get_book", "list_shelves", "list_shared_shelves", "get_shared_shelf", "reading_overview", "get_table_of_contents", "read_section", "search_in_book"]);
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    assert_eq!(v["id"], "a");

    let (status, v) = client.post(&[], json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::OK, Some(-32601)));
    let (_, v) = client.post(&[], json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "delete_everything" } })).await;
    assert_eq!(v["error"]["code"], -32602);
}

#[tokio::test]
async fn speaks_the_per_request_generation() {
    let (client, _, _) = setup().await;
    let meta = json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} });
    let request = |method: &str, mut params: Value| {
        params["_meta"] = meta.clone();
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    };
    let version = ("mcp-protocol-version", "2026-07-28");

    let (status, v) = client.post(&[version, ("mcp-method", "server/discover")], request("server/discover", json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["result"]["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(v["result"]["resultType"], "complete");
    // Discovery and the tool list must carry caching hints.
    assert_eq!((v["result"]["ttlMs"].as_u64(), v["result"]["cacheScope"].as_str()), (Some(3_600_000), Some("public")));
    assert_eq!(v["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "legejo");
    assert!(v["result"]["capabilities"]["tools"].is_object());

    let (status, v) = client.post(&[version, ("mcp-method", "tools/list")], request("tools/list", json!({}))).await;
    assert_eq!((status, v["result"]["tools"].as_array().unwrap().len()), (StatusCode::OK, 9));
    assert_eq!((v["result"]["resultType"].as_str(), v["result"]["cacheScope"].as_str()), (Some("complete"), Some("public")));
    assert!(v["result"]["ttlMs"].is_u64());
    // This generation has no ping.
    let (status, v) = client.post(&[version, ("mcp-method", "ping")], request("ping", json!({}))).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::NOT_FOUND, Some(-32601)));

    // The headers must agree with the body.
    let (status, v) = client.post(&[version], request("tools/list", json!({}))).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::BAD_REQUEST, Some(-32020)));
    let (status, v) = client.post(&[version, ("mcp-method", "tools/call")], request("tools/list", json!({}))).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::BAD_REQUEST, Some(-32020)));
    let (status, v) = client.post(&[("mcp-method", "tools/list")], request("tools/list", json!({}))).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::BAD_REQUEST, Some(-32020)), "a body without the version header");
    let call = request("tools/call", json!({ "name": "list_shelves", "arguments": {} }));
    let (status, v) = client.post(&[version, ("mcp-method", "tools/call")], call.clone()).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::BAD_REQUEST, Some(-32020)), "Mcp-Name is required");
    let (status, _) = client.post(&[version, ("mcp-method", "tools/call"), ("mcp-name", "get_book")], call.clone()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, v) = client.post(&[version, ("mcp-method", "tools/call"), ("mcp-name", "list_shelves")], call.clone()).await;
    assert_eq!((status, v["result"]["isError"].as_bool(), v["result"]["resultType"].as_str()), (StatusCode::OK, Some(false), Some("complete")));
    // "=?base64?bGlzdF9zaGVsdmVz?=" is list_shelves.
    let (status, _) = client.post(&[version, ("mcp-method", "tools/call"), ("mcp-name", "=?base64?bGlzdF9zaGVsdmVz?=")], call).await;
    assert_eq!(status, StatusCode::OK);

    // A version nobody here knows: the client is told which ones work.
    let mut future = request("tools/list", json!({}));
    future["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"] = json!("2030-01-01");
    let (status, v) = client.post(&[("mcp-protocol-version", "2030-01-01"), ("mcp-method", "tools/list")], future).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::BAD_REQUEST, Some(-32022)));
    assert_eq!(v["error"]["data"]["requested"], "2030-01-01");
    assert!(v["error"]["data"]["supported"].as_array().unwrap().contains(&json!("2026-07-28")));

    let (status, v) = client.post(&[version, ("mcp-method", "resources/list")], request("resources/list", json!({}))).await;
    assert_eq!((status, v["error"]["code"].as_i64()), (StatusCode::NOT_FOUND, Some(-32601)));
}

#[tokio::test]
async fn the_tools_read_the_callers_own_library() {
    let (client, db, cookie) = setup().await;
    let app = client.app.clone();
    let added = upload(&app, &cookie, &[("gosta.epub", novel("Gösta Berlings saga")), ("jerusalem.epub", novel("Jerusalem"))], "").await;
    let gosta = added["added"][0]["id"].as_i64().unwrap();
    let jerusalem = added["added"][1]["id"].as_i64().unwrap();
    let (_, shelf) = send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": "Classics" }))).await;
    let shelf = shelf["id"].as_i64().unwrap();
    send(&app, Method::POST, "/api/books/bulk", Some(&cookie), Some(json!({ "ids": [gosta], "action": "add_to_shelf", "shelf_id": shelf }))).await;
    send(&app, Method::POST, "/api/books/bulk", Some(&cookie), Some(json!({ "ids": [jerusalem], "action": "want", "want": true }))).await;
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{gosta}/progress"), Some(&cookie), Some(json!({ "cfi": "epubcfi(/6/4!/4/2)", "percent": 0.5 }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Library.
    let all = client.call("search_books", json!({})).await;
    assert_eq!((all["total"].as_i64(), all["books"].as_array().unwrap().len()), (Some(2), 2));
    let found = client.call("search_books", json!({ "query": "jerusalem" })).await;
    assert_eq!(found["books"][0]["title"], "Jerusalem");
    assert_eq!(found["books"][0]["author"], "Selma Lagerlöf");
    assert_eq!(client.call("search_books", json!({ "status": "reading" })).await["books"][0]["id"], gosta);
    assert_eq!(client.call("search_books", json!({ "status": "want_to_read" })).await["books"][0]["id"], jerusalem);
    assert_eq!(client.call("search_books", json!({ "shelf_id": shelf })).await["total"], 1);
    assert_eq!(client.call("search_books", json!({ "limit": 1 })).await["returned"], 1);

    let book = client.call("get_book", json!({ "book_id": gosta })).await;
    assert_eq!((book["status"].as_str(), book["progress_percent"].as_f64()), (Some("reading"), Some(50.0)));
    assert_eq!(book["shelves"][0]["name"], "Classics");
    assert_eq!(client.call("list_shelves", json!({})).await["shelves"], json!([{ "id": shelf, "name": "Classics", "books": 1, "visibility": "private", "description": null }]));

    let overview = client.call("reading_overview", json!({})).await;
    assert_eq!(overview["totals"], json!({ "books": 2, "reading": 1, "finished": 0, "want_to_read": 1, "unread": 1 }));
    assert_eq!(overview["reading_now"][0]["id"], gosta);
    assert_eq!(overview["want_to_read"][0]["id"], jerusalem);

    // The text. The cover page has no text and is not a section.
    let toc = client.call("get_table_of_contents", json!({ "book_id": gosta })).await;
    let sections = toc["sections"].as_array().unwrap();
    assert_eq!(sections.iter().map(|s| s["title"].as_str().unwrap()).collect::<Vec<_>>(), ["The Storm", "The Harbour"]);
    assert_eq!((sections[0]["starts_at_percent"].as_f64(), sections[1]["ends_at_percent"].as_f64()), (Some(0.0), Some(100.0)));
    assert_eq!((toc["reading_position_percent"].as_f64(), toc["reading_position_section"].as_i64()), (Some(50.0), Some(0)));

    let first = client.call("read_section", json!({ "book_id": gosta, "section": 0, "max_chars": 500 })).await;
    let text = first["text"].as_str().unwrap();
    assert!(text.starts_with("The Storm\nParagraph 1 of the storm at sea."), "{text}");
    assert!(text.chars().count() <= 500 && text.ends_with("at sea."), "pieces end at a line break: {text}");
    let next = first["next_offset"].as_i64().unwrap();
    let second = client.call("read_section", json!({ "book_id": gosta, "section": 0, "offset": next, "max_chars": 500 })).await;
    assert!(second["text"].as_str().unwrap().trim_start().starts_with("Paragraph"));
    assert!(second["from_percent"].as_f64().unwrap() > 0.0);
    let last = client.call("read_section", json!({ "book_id": gosta, "section": 1 })).await;
    assert_eq!(last["next_offset"], Value::Null);
    assert_eq!(last["to_percent"].as_f64(), Some(100.0));

    let hits = client.call("search_in_book", json!({ "book_id": gosta, "query": "LIGHTHOUSE" })).await;
    assert_eq!((hits["total"].as_i64(), hits["matches"][0]["title"].as_str()), (Some(2), Some("The Harbour")));
    assert!(hits["matches"][0]["context"].as_str().unwrap().contains("lighthouse keeper saw the ship"));
    assert_eq!(client.call("search_in_book", json!({ "book_id": gosta, "query": "storm", "limit": 3 })).await["returned"], 3);

    // Mistakes come back as readable tool errors.
    assert!(client.call_error("read_section", json!({ "book_id": gosta, "section": 9 })).await.contains("sections 0 to 1"));
    assert!(client.call_error("get_book", json!({})).await.contains("book_id is required"));

    // Another user's token reaches none of it.
    let (_, bob_cookie) = add_user(&db, "bob").await;
    let (_, v) = send(&app, Method::POST, "/api/account/app-passwords", Some(&bob_cookie), Some(json!({ "name": "x" }))).await;
    let bob = Client { app: app.clone(), token: v["secret"].as_str().unwrap().to_string() };
    assert_eq!(bob.call("search_books", json!({})).await["total"], 0);
    for tool in ["get_book", "get_table_of_contents"] {
        assert_eq!(bob.call_error(tool, json!({ "book_id": gosta })).await, "not found in this library");
    }
    assert_eq!(bob.call_error("search_in_book", json!({ "book_id": gosta, "query": "storm" })).await, "not found in this library");

    // Nothing was written by any of the calls.
    let books: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books").fetch_one(&db).await.unwrap();
    assert_eq!(books, 2);
}

#[tokio::test]
async fn shared_shelves_can_be_listed_but_not_read() {
    let (client, db, cookie) = setup().await;
    let app = client.app.clone();
    let alice: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'alice'").fetch_one(&db).await.unwrap();
    let (bob, bob_cookie) = add_user(&db, "bob").await;
    let (_carol, carol_cookie) = add_user(&db, "carol").await;
    let (theirs, _) = add_book(&db, bob, "Dracula").await;
    sqlx::query("UPDATE books SET author = 'Bram Stoker', description = ?, isbn = '111' WHERE id = ?")
        .bind("A count. ".repeat(200))
        .bind(theirs)
        .execute(&db)
        .await
        .unwrap();
    // Alice has the same book already.
    let (mine, _) = add_book(&db, alice, "Dracula (min)").await;
    sqlx::query("UPDATE books SET isbn = '111' WHERE id = ?").bind(mine).execute(&db).await.unwrap();

    let shelve = |cookie: String, name: &'static str, body: Value, book: Option<i64>| {
        let (app, db) = (app.clone(), db.clone());
        async move {
            let (_, v) = send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": name }))).await;
            let id = v["id"].as_i64().unwrap();
            if let Some(book) = book {
                sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)").bind(id).bind(book).execute(&db).await.unwrap();
            }
            send(&app, Method::PUT, &format!("/api/shelves/{id}"), Some(&cookie), Some(body)).await;
        }
    };
    shelve(bob_cookie.clone(), "Gothic", json!({ "name": "Gothic", "description": "Dark things", "visibility": "restricted", "members": [alice] }), Some(theirs)).await;
    shelve(bob_cookie, "Hemlig", json!({ "name": "Hemlig", "visibility": "private" }), None).await;
    shelve(carol_cookie.clone(), "Alla", json!({ "name": "Alla", "visibility": "instance" }), None).await;
    // Shared with Carol only: not Alice's to see.
    shelve(carol_cookie, "Inte Alice", json!({ "name": "Inte Alice", "visibility": "restricted", "members": [bob] }), None).await;
    let _ = cookie;

    let listed = client.call("list_shared_shelves", json!({})).await;
    assert_eq!(
        listed["shelves"],
        json!([
            { "owner": "carol", "name": "Alla", "books": 0, "description": null, "shared_with": "everyone" },
            { "owner": "bob", "name": "Gothic", "books": 1, "description": "Dark things", "shared_with": "you" },
        ])
    );

    let shelf = client.call("get_shared_shelf", json!({ "owner": "bob", "name": "Gothic" })).await;
    assert_eq!((shelf["shared_with"].as_str(), shelf["books"].as_array().unwrap().len()), (Some("you"), 1));
    let book = &shelf["books"][0];
    assert_eq!((book["title"].as_str(), book["author"].as_str(), book["in_your_library"].as_bool()), (Some("Dracula"), Some("Bram Stoker"), Some(true)));
    assert!(book["description"].as_str().unwrap().chars().count() <= 600);
    assert!(book.get("id").is_none(), "no id: the book is not the caller's to read");

    // Not shared with the caller, and the text of a shared book stays out of reach.
    assert_eq!(client.call_error("get_shared_shelf", json!({ "owner": "bob", "name": "Hemlig" })).await, "not found in this library");
    assert_eq!(client.call_error("get_shared_shelf", json!({ "owner": "carol", "name": "Inte Alice" })).await, "not found in this library");
    assert_eq!(client.call_error("get_table_of_contents", json!({ "book_id": theirs })).await, "not found in this library");
    assert_eq!(client.call_error("get_book", json!({ "book_id": theirs })).await, "not found in this library");
}
