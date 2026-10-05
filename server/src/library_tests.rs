//! API tests for want-to-read, bulk actions, zip archives, exports, account
//! deletion, publication dates, KOReader sync, OPDS auth and duplicate
//! detection on upload.

use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

pub(crate) fn epub(title: &str, identifier: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("mimetype", stored).unwrap();
    z.write_all(b"application/epub+zip").unwrap();
    z.start_file("META-INF/container.xml", stored).unwrap();
    z.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    z.start_file("c.opf", stored).unwrap();
    write!(z, r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:identifier id="i">{identifier}</dc:identifier></metadata><manifest><item id="x" href="x.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="x"/></spine></package>"#).unwrap();
    z.start_file("x.xhtml", stored).unwrap();
    z.write_all(br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>x</p></body></html>"#).unwrap();
    z.finish().unwrap().into_inner()
}

pub(crate) async fn upload(app: &axum::Router, cookie: &str, files: &[(&str, Vec<u8>)], query: &str) -> Value {
    let boundary = "legejo-test-boundary";
    let mut body = Vec::new();
    for (name, bytes) in files {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: application/epub+zip\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/books{query}"))
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn duplicates_are_caught_on_upload() {
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let a = epub("Röda rummet", "urn:isbn:9789100000001");
    let v = upload(&app, &cookie, &[("rr.epub", a.clone())], "").await;
    assert_eq!(v["added"].as_array().unwrap().len(), 1);
    assert_eq!(v["duplicates"].as_array().unwrap().len(), 0);
    let first = v["added"][0]["id"].as_i64().unwrap();

    // The same file again: not added, reported as same_file.
    let v = upload(&app, &cookie, &[("rr-igen.epub", a.clone())], "").await;
    assert_eq!(v["added"].as_array().unwrap().len(), 0);
    assert_eq!(v["duplicates"][0]["kind"], "same_file");
    assert_eq!(v["duplicates"][0]["existing_id"], first);

    // Another file with the same ISBN: added, but flagged.
    let b = epub("Röda rummet (annan utgåva)", "urn:isbn:9789100000001");
    let v = upload(&app, &cookie, &[("rr2.epub", b)], "").await;
    assert_eq!(v["added"].as_array().unwrap().len(), 1);
    assert_eq!(v["duplicates"][0]["kind"], "same_isbn");
    assert_eq!(v["duplicates"][0]["existing_id"], first);

    // "Upload anyway" overrides the same-file check.
    let v = upload(&app, &cookie, &[("rr-igen.epub", a)], "?allow_duplicates=1").await;
    assert_eq!(v["added"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn want_to_read_and_bulk_actions() {
    let (app, db, state) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let (bob, _) = add_user(&db, "bob").await;
    let (b1, u1) = add_book(&db, alice, "Ett").await;
    let (b2, u2) = add_book(&db, alice, "Två").await;
    let (bobs, _) = add_book(&db, bob, "Bobs").await;
    for u in [&u1, &u2] {
        std::fs::write(state.data_dir.join("books").join(format!("{u}.epub")), format!("bytes of {u}")).unwrap();
    }

    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{b1}/want"), Some(&cookie), Some(json!({ "want": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&cookie), None).await;
    let one = list.as_array().unwrap().iter().find(|b| b["id"] == b1).unwrap().clone();
    assert_eq!(one["want_to_read"], true);
    assert!(one["wanted_at"].is_string());
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{bobs}/want"), Some(&cookie), Some(json!({ "want": true }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "not someone else's book");

    // Shelve both (and try Bob's, which is ignored).
    let (_, shelf) = send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": "Hög" }))).await;
    let shelf = shelf["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::POST, "/api/books/bulk", Some(&cookie),
        Some(json!({ "ids": [b1, b2, bobs], "action": "add_to_shelf", "shelf_id": shelf }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["done"], 2);
    let (_, s) = send(&app, Method::GET, &format!("/api/shelves/{shelf}"), Some(&cookie), None).await;
    assert_eq!(s["books"].as_array().unwrap().len(), 2);

    let (_, v) = send(&app, Method::POST, "/api/books/bulk", Some(&cookie),
        Some(json!({ "ids": [b1, b2], "action": "add_tag", "tag": "deckare" }))).await;
    assert_eq!(v["done"], 2);
    let (_, d) = send(&app, Method::GET, &format!("/api/books/{b2}"), Some(&cookie), None).await;
    assert_eq!(d["tags"], json!(["deckare"]));
    send(&app, Method::POST, "/api/books/bulk", Some(&cookie),
        Some(json!({ "ids": [b1, b2], "action": "add_tag", "tag": "Deckare" }))).await;
    let (_, d) = send(&app, Method::GET, &format!("/api/books/{b2}"), Some(&cookie), None).await;
    assert_eq!(d["tags"].as_array().unwrap().len(), 1, "case-insensitively once");

    let (_, v) = send(&app, Method::POST, "/api/books/bulk", Some(&cookie), Some(json!({ "ids": [b1, b2], "action": "want", "want": false }))).await;
    assert_eq!(v["done"], 2);

    // The archive holds both files.
    let req = Request::builder().uri(format!("/api/books/archive?ids={b1},{b2},{bobs}")).header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    assert_eq!(zip.len(), 2);

    // Delete both; Bob's survives.
    let (_, v) = send(&app, Method::POST, "/api/books/bulk", Some(&cookie), Some(json!({ "ids": [b1, b2, bobs], "action": "delete" }))).await;
    assert_eq!(v["done"], 2);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books").fetch_one(&db).await.unwrap();
    assert_eq!(left, 1);
    assert!(!state.data_dir.join("books").join(format!("{u1}.epub")).exists());

    let (_, log) = send(&app, Method::GET, "/api/admin/log", Some(&cookie), None).await;
    let _ = log; // alice is not an admin; the audit log is tested in invite.rs
}

#[tokio::test]
async fn export_metadata_and_whole_library() {
    let (app, db, state) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let v = upload(&app, &cookie, &[("a.epub", epub("Röda rummet", "id-a")), ("b.epub", epub("Hemsöborna", "id-b"))], "").await;
    let first = v["added"][0]["id"].as_i64().unwrap();
    send(&app, Method::PUT, &format!("/api/books/{first}"), Some(&cookie),
        Some(json!({ "title": "Röda rummet", "author": "Strindberg, August", "tags": ["klassiker"] }))).await;

    // Metadata export is synchronous.
    let req = Request::builder().uri("/api/account/metadata?format=csv").header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let csv = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert!(csv.starts_with('\u{feff}'));
    assert!(csv.contains("\"Strindberg, August\""));
    assert!(csv.contains("books/Strindberg, August - Röda rummet.epub"));
    let (_, json) = send(&app, Method::GET, "/api/account/metadata?format=json", Some(&cookie), None).await;
    assert_eq!(json["books"].as_array().unwrap().len(), 2);
    assert_eq!(json["format"], "legejo-export-1");

    // The whole library: queued, built by the worker, downloadable in ranges.
    let (status, v) = send(&app, Method::POST, "/api/account/export", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(v["status"], "queued");
    let (status, _) = send(&app, Method::POST, "/api/account/export", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "one at a time");
    crate::export::run_pending(&state, "test-worker").await;
    let (_, v) = send(&app, Method::GET, "/api/account/export", Some(&cookie), None).await;
    assert_eq!(v["status"], "done", "{v}");
    assert_eq!(v["done"], 2);
    let id = v["id"].as_i64().unwrap();
    let part = v["parts"][0]["name"].as_str().unwrap().to_string();
    let size = v["parts"][0]["size"].as_i64().unwrap();

    let req = Request::builder().uri(format!("/api/account/export/{id}/{part}")).header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    assert_eq!(bytes.len() as i64, size);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
    assert!(names.contains(&"README.txt".to_string()));
    assert!(names.contains(&"legejo.csv".to_string()));
    assert!(names.iter().filter(|n| n.starts_with("books/") && n.ends_with(".epub")).count() == 2, "{names:?}");

    // Resumable: a Range request gets 206 with just that slice.
    let req = Request::builder()
        .uri(format!("/api/account/export/{id}/{part}"))
        .header(header::COOKIE, &cookie)
        .header(header::RANGE, "bytes=10-19")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().len(), 10);

    // Someone else can't fetch it.
    let (_, bob_cookie) = add_user(&db, "bob").await;
    let req = Request::builder().uri(format!("/api/account/export/{id}/{part}")).header(header::COOKIE, &bob_cookie).body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::NOT_FOUND);
    let _ = alice;
}

#[tokio::test]
async fn deleting_an_account_takes_everything() {
    let (app, db, state) = test_app().await;
    let hash = crate::db::hash_password("rätt lösenord").unwrap();
    let (alice, cookie) = add_user(&db, "alice").await;
    let (admin, admin_cookie) = add_user(&db, "chef").await;
    for id in [alice, admin] {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(&hash).bind(id).execute(&db).await.unwrap();
    }
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin).execute(&db).await.unwrap();
    let v = upload(&app, &cookie, &[("a.epub", epub("Röda rummet", "id-a"))], "").await;
    let uuid = v["added"][0]["uuid"].as_str().unwrap().to_string();
    send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": "Min hylla" }))).await;

    let (status, _) = send(&app, Method::DELETE, "/api/account", Some(&cookie), Some(json!({ "password": "fel" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&app, Method::DELETE, "/api/account", Some(&admin_cookie), Some(json!({ "password": "rätt lösenord" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "the last admin stays");

    let (status, _) = send(&app, Method::DELETE, "/api/account", Some(&cookie), Some(json!({ "password": "rätt lösenord" }))).await;
    assert_eq!(status, StatusCode::OK);
    for (table, col) in [("users", "id"), ("books", "owner_id"), ("shelves", "owner_id"), ("sessions", "user_id")] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {col} = ?")).bind(alice).fetch_one(&db).await.unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert!(!state.data_dir.join("books").join(format!("{uuid}.epub")).exists());
    let (status, _) = send(&app, Method::GET, "/api/books", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn edition_date_and_first_published_year() {
    let (app, db, state) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let (book, _) = add_book(&db, alice, "Röda rummet").await;
    let uri = format!("/api/books/{book}");

    // Normalized on the way in; nonsense refused.
    let (status, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet", "published": "[2013]", "first_published": 1879 }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["published"], "2013");
    assert_eq!(v["first_published"], 1879);
    let (status, _) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet", "published": "någon gång" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet", "first_published": 3000 }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // Absent leaves the year alone; null clears it.
    let (_, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet", "published": "2013-10-25" }))).await;
    assert_eq!((v["published"].clone(), v["first_published"].clone()), (json!("2013-10-25"), json!(1879)));
    let (_, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet", "first_published": null }))).await;
    assert_eq!(v["first_published"], Value::Null);

    // The one-off backfill: normalize, guess the year unless after the author's death.
    let (a, _) = add_book(&db, alice, "Gammal").await;
    let (b, _) = add_book(&db, alice, "Postum").await;
    let (c, _) = add_book(&db, alice, "Platshållare").await;
    sqlx::query("UPDATE books SET published = '1891-01-01T00:00:00+00:00' WHERE id = ?").bind(a).execute(&db).await.unwrap();
    sqlx::query("UPDATE books SET published = '2013', author_death_year = 1912 WHERE id = ?").bind(b).execute(&db).await.unwrap();
    sqlx::query("UPDATE books SET published = '0101-01-01T00:00:00+00:00' WHERE id = ?").bind(c).execute(&db).await.unwrap();
    crate::pubdate::backfill(state.clone()).await;
    let row = |id: i64| {
        let db = db.clone();
        async move {
            sqlx::query_as::<_, (Option<String>, Option<i64>)>("SELECT published, first_published FROM books WHERE id = ?")
                .bind(id)
                .fetch_one(&db)
                .await
                .unwrap()
        }
    };
    assert_eq!(row(a).await, (Some("1891-01-01".into()), Some(1891)));
    assert_eq!(row(b).await, (Some("2013".into()), None), "edition after the author's death");
    assert_eq!(row(c).await, (None, None), "calibre's placeholder");
    // It runs once.
    sqlx::query("UPDATE books SET published = '[1999]' WHERE id = ?").bind(c).execute(&db).await.unwrap();
    crate::pubdate::backfill(state.clone()).await;
    assert_eq!(row(c).await.0.as_deref(), Some("[1999]"));
}

#[tokio::test]
async fn koreader_sync() {
    use md5::{Digest, Md5};
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let v = upload(&app, &cookie, &[("a.epub", epub("Röda rummet", "id-a"))], "").await;
    let book = v["added"][0]["id"].as_i64().unwrap();
    let document: String = sqlx::query_scalar("SELECT koreader_md5 FROM books WHERE id = ?").bind(book).fetch_one(&db).await.unwrap();
    assert_eq!(document.len(), 32);

    let (_, k) = send(&app, Method::POST, "/api/account/kosync-key", Some(&cookie), None).await;
    let key = k["key"].as_str().unwrap().to_string();
    let auth_key: String = Md5::digest(key.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();

    let ko = |method: Method, uri: &str, user: &str, key: &str, body: Option<Value>| {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/kosync{uri}"))
            .header("accept", "application/vnd.koreader.v1+json")
            .header("x-auth-user", user)
            .header("x-auth-key", key);
        let body = match body {
            Some(b) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let req = req.body(body).unwrap();
        let app = app.clone();
        async move {
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null))
        }
    };

    assert_eq!(ko(Method::GET, "/users/auth", "Alice", &auth_key, None).await.0, StatusCode::OK);
    assert_eq!(ko(Method::GET, "/users/auth", "alice", "0123456789abcdef0123456789abcdef", None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(ko(Method::POST, "/users/create", "x", "y", Some(json!({ "username": "x", "password": "y" }))).await.0, StatusCode::PAYMENT_REQUIRED);

    // Nothing yet: an empty object, as koreader-sync-server answers.
    let (status, v) = ko(Method::GET, &format!("/syncs/progress/{document}"), "alice", &auth_key, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({})));

    let (status, v) = ko(Method::PUT, "/syncs/progress", "alice", &auth_key, Some(json!({
        "document": document, "progress": "/body/DocFragment[12]/body/p[3]/text().0", "percentage": 0.37,
        "device": "Boox Note Air", "device_id": "ABC123"
    }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["timestamp"].as_i64().unwrap() > 0);

    let (_, v) = ko(Method::GET, &format!("/syncs/progress/{document}"), "alice", &auth_key, None).await;
    assert_eq!(v["progress"], "/body/DocFragment[12]/body/p[3]/text().0");
    assert_eq!(v["percentage"], 0.37);
    assert_eq!(v["device"], "Boox Note Air");

    // Legejo shows it as the book's progress; the web reader gets the percent, no CFI.
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&cookie), None).await;
    let b = list.as_array().unwrap().iter().find(|b| b["id"] == book).unwrap().clone();
    assert_eq!(b["progress_percent"], 0.37);
    assert!(b["last_read_at"].is_string());
    let (_, p) = send(&app, Method::GET, &format!("/api/books/{book}/progress"), Some(&cookie), None).await;
    assert_eq!((p["cfi"].clone(), p["percent"].clone()), (Value::Null, json!(0.37)));

    // Reading on the web afterwards wins again.
    send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&cookie), Some(json!({ "cfi": "epubcfi(/6/4)", "percent": 0.5 }))).await;
    let (_, p) = send(&app, Method::GET, &format!("/api/books/{book}/progress"), Some(&cookie), None).await;
    assert_eq!((p["cfi"].clone(), p["percent"].clone()), (json!("epubcfi(/6/4)"), json!(0.5)));

    // A file Legejo doesn't know still syncs between KOReader devices.
    let other = "ffffffffffffffffffffffffffffffff";
    ko(Method::PUT, "/syncs/progress", "alice", &auth_key, Some(json!({ "document": other, "progress": "12", "percentage": 0.1 }))).await;
    let (_, v) = ko(Method::GET, &format!("/syncs/progress/{other}"), "alice", &auth_key, None).await;
    assert_eq!(v["percentage"], 0.1);

    // Removing the key ends access.
    send(&app, Method::DELETE, "/api/account/kosync-key", Some(&cookie), None).await;
    assert_eq!(ko(Method::GET, "/users/auth", "alice", &auth_key, None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn opds_takes_the_account_password_and_app_passwords() {
    use base64::Engine;
    let (app, db, _) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let hash = crate::db::hash_password("kontots lösenord").unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(&hash).bind(alice).execute(&db).await.unwrap();
    let opds = |user: &str, pass: &str| {
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        let req = Request::builder().uri("/api/opds").header(header::AUTHORIZATION, format!("Basic {auth}")).body(Body::empty()).unwrap();
        let app = app.clone();
        async move { app.oneshot(req).await.unwrap().status() }
    };

    // The account password still works.
    assert_eq!(opds("alice", "kontots lösenord").await, StatusCode::OK);
    assert_eq!(opds("alice", "fel").await, StatusCode::UNAUTHORIZED);

    let (status, v) = send(&app, Method::POST, "/api/account/app-passwords", Some(&cookie), Some(json!({ "name": "KOReader på Boxen" }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let secret = v["secret"].as_str().unwrap().to_string();
    let id = v["id"].as_i64().unwrap();
    assert_eq!(opds("alice", &secret).await, StatusCode::OK);
    // Forgiving about hyphens and case, as typed on an e-reader.
    assert_eq!(opds("Alice", &secret.replace('-', "").to_uppercase()).await, StatusCode::OK);
    // Only for its own account.
    let (bob, _) = add_user(&db, "bob").await;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(&hash).bind(bob).execute(&db).await.unwrap();
    assert_eq!(opds("bob", &secret).await, StatusCode::UNAUTHORIZED);

    let (_, list) = send(&app, Method::GET, "/api/account/app-passwords", Some(&cookie), None).await;
    assert_eq!(list[0]["name"], "KOReader på Boxen");
    assert!(list[0]["last_used_at"].is_string(), "use is recorded");
    assert!(list[0].get("secret").is_none(), "the secret is never shown again");

    send(&app, Method::DELETE, &format!("/api/account/app-passwords/{id}"), Some(&cookie), None).await;
    assert_eq!(opds("alice", &secret).await, StatusCode::UNAUTHORIZED);
    assert_eq!(opds("alice", "kontots lösenord").await, StatusCode::OK);
}
