//! Highlights and notes from a Kobo: the device is pointed here once the
//! admin setting is on, is known by its id from its syncs, what it sends for
//! a book lands among the owner's annotations, and what it asks for comes
//! back in its own shape, so that it keeps what it has.

use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

const DEVICE: &str = "d9f26b8441cbf1519a6cf89b1a2fbd864dbfbd35e1db395178965909c06683a7";
const OTHER_DEVICE: &str = "0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9";

/// A request as the device makes it: its id in a header, no cookie.
async fn from_device_full(app: &Router, method: Method, uri: &str, device: &str, body: Option<Value>) -> (StatusCode, HeaderMap, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-kobo-deviceid", device)
        .header("x-kobo-devicemodel", "Kobo Clara 2E")
        .header(header::CONTENT_TYPE, "application/json");
    let req = match body {
        Some(body) => req.body(Body::from(body.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn from_device(app: &Router, method: Method, uri: &str, device: &str, body: Option<Value>) -> (StatusCode, Value) {
    let (status, _, v) = from_device_full(app, method, uri, device, body).await;
    (status, v)
}

async fn kobo_token(app: &Router, cookie: &str) -> String {
    let (status, v) = send(app, Method::POST, "/api/account/kobo-token", Some(cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    v["kobo_token"].as_str().unwrap().to_string()
}

async fn turn_on(app: &Router, db: &sqlx::AnyPool, admin_cookie: &str) {
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(db).await.unwrap();
    let body = json!({ "libris_enabled": true, "registration_enabled": false, "kobo_annotations_enabled": true });
    let (status, v) = send(app, Method::PUT, "/api/admin/settings", Some(admin_cookie), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["kobo_annotations_enabled"], true);
}

/// A book with a uuid of the kind the device sees, with dashes added.
async fn add_kobo_book(db: &sqlx::AnyPool, owner: i64, title: &str, uuid: &str) -> (i64, String) {
    let id: i64 = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, ?, 1) RETURNING id")
        .bind(uuid)
        .bind(owner)
        .bind(title)
        .fetch_one(db)
        .await
        .unwrap();
    let dashed = format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32]);
    (id, dashed)
}

fn patch(annotations: Value, deleted: Value) -> Value {
    json!({ "updatedAnnotations": annotations, "deletedAnnotationIds": deleted })
}

fn highlight() -> Value {
    json!({
        "clientLastModifiedUtc": "2026-10-10T15:43:14Z", "highlightColor": "#F6F3B3", "highlightedText": "twinkling", "id": "ffc65723-4fd2-425a-961a-f8b37f270c34",
        "location": { "span": { "chapterFilename": "OEBPS/xhtml/09_WITCH_WAR.xhtml", "chapterProgress": 0.157, "chapterTitle": "WITCH WAR",
                                "endChar": 42, "endPath": "span#kobo\\.10\\.11", "startChar": 33, "startPath": "span#kobo\\.10\\.11" } },
        "type": "highlight" })
}

fn note() -> Value {
    json!({
        "clientLastModifiedUtc": "2026-10-10T15:43:26Z", "highlightColor": "#C6E09E", "highlightedText": "Smooth", "id": "e7b9c4f4-dbf9-4626-b414-00c68c1b0db4",
        "location": { "span": { "chapterFilename": "OEBPS/xhtml/09_WITCH_WAR.xhtml", "chapterProgress": 0.157, "chapterTitle": "WITCH WAR",
                                "endChar": 6, "endPath": "span#kobo\\.11\\.2", "startChar": 0, "startPath": "span#kobo\\.11\\.2" } },
        "noteText": "Mo0s", "type": "note" })
}

#[tokio::test]
async fn off_by_default_the_device_is_not_told_about_us_and_keeps_its_highlights() {
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let token = kobo_token(&app, &cookie).await;
    let (status, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["Resources"]["image_host"].is_string());
    assert!(v["Resources"]["reading_services_host"].is_null());
    // The highlights are refused, never answered as "none".
    let (status, _) = from_device(&app, Method::GET, "/api/v3/content/abc/annotations?limit=100", DEVICE, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = from_device(&app, Method::PATCH, "/api/v3/content/abc/annotations", DEVICE, Some(patch(json!([highlight()]), json!([])))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for uri in ["/api/UserStorage/Metadata", "/api/internal/notebooks", "/api/v3/content/checkforchanges"] {
        let (status, _) = from_device(&app, Method::POST, uri, DEVICE, Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn what_the_device_sends_for_a_book_lands_among_the_owners_annotations_and_comes_back_to_it() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (bob, _) = add_user(&db, "bob").await;
    turn_on(&app, &db, &a).await;
    let token = kobo_token(&app, &a).await;
    let (book, content) = add_kobo_book(&db, alice, "Röda rummet", "41069f5ea598afe0ec43e1eec0e97766").await;
    let (_, bobs_content) = add_kobo_book(&db, bob, "Bobs bok", "00e38331f90ab19defcbd5f61d35fa76").await;
    let uri = format!("/api/v3/content/{content}/annotations");

    // The device is pointed here, and is known from any request with the token that names it.
    let (status, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let host = v["Resources"]["reading_services_host"].as_str().unwrap();
    assert!(!host.ends_with('/') && !host.contains("/api/"), "{host}");
    let known = || sqlx::query_as::<_, (i64, Option<String>)>("SELECT user_id, model FROM kobo_devices WHERE device_id = ?").bind(DEVICE).fetch_optional(&db);
    assert_eq!(known().await.unwrap(), None, "the initialization did not name the device");
    let (status, _) = from_device(&app, Method::GET, &format!("/api/kobo/{token}/v1/user/profile"), DEVICE, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(known().await.unwrap(), Some((alice, Some("Kobo Clara 2E".into()))));

    // Nothing yet: nothing has changed, and an empty list in the device's shape, with an ETag.
    let asks = json!([{ "ContentId": content, "etag": "W/\"0\"" }, { "ContentId": bobs_content, "etag": "W/\"0\"" }]);
    let (status, v) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!((status, v), (StatusCode::OK, json!([])));
    let (status, headers, v) = from_device_full(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!((status, &v), (StatusCode::OK, &json!({ "annotations": [], "nextPageOffsetToken": null })));
    let etag_of_none = headers.get(header::ETAG).unwrap().to_str().unwrap().to_string();
    assert!(etag_of_none.starts_with('"'), "{etag_of_none}");
    // The other paths answer with what the device takes as "nothing here".
    let (status, v) = from_device(&app, Method::GET, "/api/internal/notebooks", DEVICE, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({ "data": [], "totalResults": 0 })));
    let (status, v) = from_device(&app, Method::GET, "/api/UserStorage/Metadata", DEVICE, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({ "continuationToken": null, "metadata": [] })));

    // A highlight and a note, as the device sends them: taken in, acknowledged without a body.
    let (status, v) = from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([highlight(), note()]), json!([])))).await;
    assert_eq!((status, v), (StatusCode::NO_CONTENT, Value::Null));
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[0]["source"], "kobo");
    assert_eq!(list[0]["text"], "twinkling");
    assert!(list[0]["cfi"].is_null(), "not placed in the EPUB yet");
    assert!(list[0]["location"].as_str().unwrap().contains("09_WITCH_WAR.xhtml"));
    assert_eq!(list[1]["note"], "Mo0s");
    assert_eq!(list[1]["text"], "Smooth");

    // The book has changed since the device was last handed its list; once handed, no longer.
    let (_, v) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(v, json!([content]));
    // Asked for, they come back as sent: the device then keeps them.
    let (status, headers, v) = from_device_full(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([]));
    assert_ne!(headers.get(header::ETAG).unwrap().to_str().unwrap(), etag_of_none);
    let back = v["annotations"].as_array().unwrap();
    assert_eq!(back.len(), 2, "{v}");
    assert_eq!(back[0]["id"], highlight()["id"]);
    assert_eq!(back[0]["highlightedText"], "twinkling");
    assert_eq!(back[0]["highlightColor"], "#F6F3B3");
    assert_eq!(back[0]["type"], "highlight");
    assert_eq!(back[0]["clientLastModifiedUtc"], "2026-10-10T15:43:14Z", "unchanged here: the device's own time");
    assert_eq!(back[0]["location"], highlight()["location"]);
    assert_eq!(back[0]["attachments"], json!({}));
    assert!(back[0].get("context").is_some_and(Value::is_null));
    assert!(back[0].get("noteText").is_none());
    assert_eq!(back[1]["noteText"], "Mo0s");
    assert_eq!(back[1]["type"], "note");
    assert!(v["nextPageOffsetToken"].is_null());

    // A note written here: the book is reported as changed, and the note goes back with a later time.
    let web_id = list[0]["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::PUT, &format!("/api/annotations/{web_id}"), Some(&a), Some(json!({ "note": "Blinkar" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"][0]["noteText"], "Blinkar");
    assert_eq!(v["annotations"][0]["type"], "note");
    let served_time = v["annotations"][0]["clientLastModifiedUtc"].as_str().unwrap().to_string();
    assert_ne!(served_time, "2026-10-10T15:43:14Z");
    // The device hands back its old copy, not yet brought up to date: the note here stands.
    let (status, _) = from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([highlight()]), json!([])))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list[0]["note"], "Blinkar");
    // The device hands back what it was served: nothing changes, and the exchange settles.
    let mut echo = highlight();
    echo["noteText"] = json!("Blinkar");
    echo["type"] = json!("note");
    echo["clientLastModifiedUtc"] = json!(served_time);
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([echo]), json!([])))).await;
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"][0]["noteText"], "Blinkar");
    assert_eq!(v["annotations"][0]["clientLastModifiedUtc"], served_time);
    // One removed here goes back without.
    let (status, v) = send(&app, Method::PUT, &format!("/api/annotations/{web_id}"), Some(&a), Some(json!({ "note": null }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert!(v["annotations"][0].get("noteText").is_none(), "{v}");
    assert_eq!(v["annotations"][0]["type"], "highlight");

    // The same note again, changed on the device, dated anew by it: one row, updated.
    let mut changed = note();
    changed["noteText"] = json!("Moose");
    changed["clientLastModifiedUtc"] = json!("2026-10-10T15:50:00Z");
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([changed.clone()]), json!([])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[1]["note"], "Moose");
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"][1]["noteText"], "Moose");
    assert_eq!(v["annotations"][1]["clientLastModifiedUtc"], "2026-10-10T15:50:00Z", "as the device dated it");

    // The reader places it; the place stays through the next update from the device.
    let placed = list[1]["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::PUT, &format!("/api/annotations/{placed}"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/20!/4/12,/1:0,/1:6)" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["cfi"], "epubcfi(/6/20!/4/12,/1:0,/1:6)");
    assert_eq!(v["note"], "Moose", "the note is not touched by placing");
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([changed]), json!([])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list[1]["cfi"], "epubcfi(/6/20!/4/12,/1:0,/1:6)");

    // Removed on the device: removed here. Removed here: no longer handed back.
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([]), json!([highlight()["id"].as_str().unwrap()])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["text"], "Smooth");
    let (status, _) = send(&app, Method::DELETE, &format!("/api/annotations/{placed}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]), "gone here: the device is to fetch and find none");
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v, json!({ "annotations": [], "nextPageOffsetToken": null }));
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([]));

    // A device nobody has synced with is told to come back, so that it keeps what it has.
    let (status, _) = from_device(&app, Method::PATCH, &uri, "unknown-device", Some(patch(json!([highlight()]), json!([])))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), "unknown-device", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, v) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", "unknown-device", Some(json!([]))).await;
    assert_eq!((status, v), (StatusCode::OK, json!([])));
    // A book that is not the user's is not there.
    let (status, _) = from_device(&app, Method::PATCH, &format!("/api/v3/content/{bobs_content}/annotations"), DEVICE, Some(patch(json!([highlight()]), json!([])))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = from_device(&app, Method::GET, &format!("/api/v3/content/{bobs_content}/annotations?limit=100"), DEVICE, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations").fetch_one(&db).await.unwrap();
    assert_eq!(total, 0);

    // Under the token too, for a device that keeps the path: known from that very request.
    let (status, v) = from_device(&app, Method::PATCH, &format!("/api/kobo/{token}/api/v3/content/{content}/annotations"), OTHER_DEVICE, Some(patch(json!([highlight()]), json!([])))).await;
    assert_eq!((status, v), (StatusCode::NO_CONTENT, Value::Null));
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations").fetch_one(&db).await.unwrap();
    assert_eq!(total, 1);
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), OTHER_DEVICE, None).await;
    assert_eq!(v["annotations"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_highlight_made_in_the_web_reader_goes_to_the_device_placed_by_kepub_spans() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    turn_on(&app, &db, &a).await;
    let token = kobo_token(&app, &a).await;
    let (book, content) = add_kobo_book(&db, alice, "Alice", "5f1c2a9d3b4e4c6f8a7b9c0d1e2f3a4b").await;
    // The KEPUB the device reads, as kepubify leaves it in the cache.
    let dir = state.data_dir.join("kepub");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("5f1c2a9d3b4e4c6f8a7b9c0d1e2f3a4b.kepub.epub"), crate::kobospan::sample_kepub()).await.unwrap();
    let uri = format!("/api/v3/content/{content}/annotations");
    let asks = json!([{ "ContentId": content, "etag": "W/\"0\"" }]);
    from_device(&app, Method::GET, &format!("/api/kobo/{token}/v1/user/profile"), DEVICE, None).await;
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"].as_array().unwrap().len(), 0);

    // A note made in the web reader: the device is told the book changed, and finds it placed by spans.
    let made = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/2,/1:15,/1:25)", "text": "said  Alice", "note": "Hon svarar" });
    let (status, v) = send(&app, Method::POST, &format!("/api/books/{book}/annotations"), Some(&a), Some(made.clone())).await;
    assert!(status.is_success(), "{status} {v}");
    let web_id = v["id"].as_i64().unwrap();
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    let served = v["annotations"][0].clone();
    assert_eq!(v["annotations"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(served["highlightedText"], "said Alice");
    assert_eq!(served["noteText"], "Hon svarar");
    assert_eq!(served["type"], "note");
    assert_eq!(served["location"]["span"]["chapterFilename"], "OEBPS/xhtml/ch1.xhtml");
    assert_eq!(served["location"]["span"]["startPath"], "span#kobo\\.4\\.1");
    assert_eq!(served["location"]["span"]["startChar"], 15);
    assert_eq!(served["location"]["span"]["endPath"], "span#kobo\\.4\\.1");
    assert_eq!(served["location"]["span"]["endChar"], 25);
    assert_eq!(served["location"]["span"]["chapterTitle"], "Chapter X: The Lobster Quadrille");
    let kobo_id = served["id"].as_str().unwrap().to_string();
    assert_eq!((kobo_id.len(), kobo_id.matches('-').count()), (36, 4), "{kobo_id}");
    assert_eq!(served["clientLastModifiedUtc"].as_str().unwrap().len(), "2026-10-10T17:22:10Z".len());
    assert_eq!(served["attachments"], json!({}));
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([]));
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list[0]["source"], "web", "still the web's own");
    assert_eq!(list[0]["cfi"], made["cfi"]);

    // The device hands it back as served: nothing changes.
    let (status, _) = from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([served]), json!([])))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([]));
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!((list.as_array().unwrap().len(), list[0]["note"].as_str()), (1, Some("Hon svarar")));

    // Removed on the device: removed here.
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([]), json!([kobo_id])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
    let (status, _) = send(&app, Method::GET, &format!("/api/annotations/{web_id}"), Some(&a), None).await;
    assert_ne!(status, StatusCode::OK);

    // Made again, then removed in the web: the device is told, and finds none.
    let (_, v) = send(&app, Method::POST, &format!("/api/books/{book}/annotations"), Some(&a), Some(made)).await;
    let web_id = v["id"].as_i64().unwrap();
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"].as_array().unwrap().len(), 1);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/annotations/{web_id}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks.clone())).await;
    assert_eq!(changed, json!([content]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"].as_array().unwrap().len(), 0);

    // One whose text is not in the KEPUB stays with the web alone.
    let elsewhere = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/2,/1:0,/1:5)", "text": "not in the book" });
    let (status, _) = send(&app, Method::POST, &format!("/api/books/{book}/annotations"), Some(&a), Some(elsewhere)).await;
    assert!(status.is_success());
    let (_, changed) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(asks)).await;
    assert_eq!(changed, json!([]));
    let (_, v) = from_device(&app, Method::GET, &format!("{uri}?limit=100"), DEVICE, None).await;
    assert_eq!(v["annotations"].as_array().unwrap().len(), 0);
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn the_kepub_a_kobo_gets_is_known_to_koreader_by_its_own_fingerprint() {
    let (_, db, state) = test_app().await;
    let (alice, _) = add_user(&db, "alice").await;
    let (book, uuid) = add_book(&db, alice, "Alice").await;
    // The KEPUB the device is sent, as kepubify leaves it in the cache.
    let dir = state.data_dir.join("kepub");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let kepub = dir.join(format!("{uuid}.kepub.epub"));
    tokio::fs::write(&kepub, crate::kobospan::sample_kepub()).await.unwrap();

    assert_eq!(crate::kobo::ensure_kepub(&state, &uuid).await.as_deref(), Some(kepub.as_path()));
    let md5 = crate::kosync::partial_md5(&kepub).unwrap();
    let known: Vec<i64> = sqlx::query_scalar("SELECT book_id FROM book_file_ids WHERE md5 = ?").bind(&md5).fetch_all(&db).await.unwrap();
    assert_eq!(known, [book]);

    // Sent again: still once.
    crate::kobo::ensure_kepub(&state, &uuid).await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM book_file_ids WHERE md5 = ?").bind(&md5).fetch_one(&db).await.unwrap();
    assert_eq!(count, 1);
}
