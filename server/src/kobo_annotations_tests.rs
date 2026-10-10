//! Highlights and notes from a Kobo: the device is pointed here once the
//! admin setting is on, is known by its id from its syncs, and what it sends
//! for a book lands among the owner's annotations.

use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

const DEVICE: &str = "d9f26b8441cbf1519a6cf89b1a2fbd864dbfbd35e1db395178965909c06683a7";

/// A request as the device makes it: its id in a header, no cookie.
async fn from_device(app: &Router, method: Method, uri: &str, device: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri).header("x-kobo-deviceid", device).header("x-kobo-devicemodel", "Kobo Clara 2E");
    let req = match body {
        Some(body) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())).unwrap(),
        None => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            req.body(Body::empty()).unwrap()
        }
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
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

#[tokio::test]
async fn off_by_default_the_device_is_not_told_about_us_and_the_root_is_not_there() {
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let token = kobo_token(&app, &cookie).await;
    let (status, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["Resources"]["image_host"].is_string());
    assert!(v["Resources"]["reading_services_host"].is_null());
    for uri in ["/api/v3/content/abc/annotations", "/api/UserStorage/Metadata", "/api/internal/notebooks", "/api/v3/content/checkforchanges"] {
        let (status, _) = from_device(&app, Method::POST, uri, DEVICE, Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn what_the_device_sends_for_a_book_lands_among_the_owners_annotations() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (bob, _) = add_user(&db, "bob").await;
    turn_on(&app, &db, &a).await;
    let token = kobo_token(&app, &a).await;
    let (book, content) = add_kobo_book(&db, alice, "Röda rummet", "41069f5ea598afe0ec43e1eec0e97766").await;
    let (_, bobs_content) = add_kobo_book(&db, bob, "Bobs bok", "00e38331f90ab19defcbd5f61d35fa76").await;

    // The device is pointed here, and is known from its sync.
    let (_, v) = from_device(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), DEVICE, None).await;
    let host = v["Resources"]["reading_services_host"].as_str().unwrap();
    assert!(!host.ends_with('/') && !host.contains("/api/"), "{host}");
    let known: Option<(i64, Option<String>)> = sqlx::query_as("SELECT user_id, model FROM kobo_devices WHERE device_id = ?").bind(DEVICE).fetch_optional(&db).await.unwrap();
    assert_eq!(known, Some((alice, Some("Kobo Clara 2E".into()))));

    // Every path the device uses answers with its empty shape.
    let none = json!({ "data": [], "totalResults": 0 });
    let (status, v) = from_device(&app, Method::GET, &format!("/api/v3/content/{content}/annotations?limit=100"), DEVICE, None).await;
    assert_eq!((status, v), (StatusCode::OK, none.clone()));
    let (status, v) = from_device(&app, Method::POST, "/api/v3/content/checkforchanges", DEVICE, Some(json!([{ "ContentId": content, "etag": "W/\"0\"" }]))).await;
    assert_eq!((status, v), (StatusCode::OK, json!([])));
    let (status, v) = from_device(&app, Method::GET, "/api/internal/notebooks", DEVICE, None).await;
    assert_eq!((status, v), (StatusCode::OK, none.clone()));
    let (status, v) = from_device(&app, Method::GET, "/api/UserStorage/Metadata", DEVICE, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({})));

    // A highlight and a note, as the device sends them.
    let highlight = json!({
        "clientLastModifiedUtc": "2026-10-10T15:43:14Z", "highlightedText": "twinkling", "id": "ffc65723-4fd2-425a-961a-f8b37f270c34",
        "location": { "span": { "chapterFilename": "OEBPS/xhtml/09_WITCH_WAR.xhtml", "chapterProgress": 0.157, "chapterTitle": "WITCH WAR",
                                "endChar": 42, "endPath": "span#kobo\\.10\\.11", "startChar": 33, "startPath": "span#kobo\\.10\\.11" } },
        "type": "highlight" });
    let note = json!({
        "clientLastModifiedUtc": "2026-10-10T15:43:26Z", "highlightedText": "Smooth", "id": "e7b9c4f4-dbf9-4626-b414-00c68c1b0db4",
        "location": { "span": { "chapterFilename": "OEBPS/xhtml/09_WITCH_WAR.xhtml", "chapterProgress": 0.157, "chapterTitle": "WITCH WAR",
                                "endChar": 6, "endPath": "span#kobo\\.11\\.2", "startChar": 0, "startPath": "span#kobo\\.11\\.2" } },
        "noteText": "Mo0s", "type": "note" });
    let uri = format!("/api/v3/content/{content}/annotations");
    let (status, v) = from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([highlight, note]), json!([])))).await;
    assert_eq!((status, v), (StatusCode::OK, none.clone()));
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[0]["source"], "kobo");
    assert_eq!(list[0]["text"], "twinkling");
    assert!(list[0]["cfi"].is_null(), "not placed in the EPUB yet");
    assert!(list[0]["location"].as_str().unwrap().contains("09_WITCH_WAR.xhtml"));
    assert_eq!(list[1]["note"], "Mo0s");
    assert_eq!(list[1]["text"], "Smooth");

    // The same note again, changed: one row, updated.
    let mut changed = note.clone();
    changed["noteText"] = json!("Moose");
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([changed]), json!([])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[1]["note"], "Moose");

    // The reader places it; the place stays through the next update from the device.
    let placed = list[1]["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::PUT, &format!("/api/annotations/{placed}"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/20!/4/12,/1:0,/1:6)" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["cfi"], "epubcfi(/6/20!/4/12,/1:0,/1:6)");
    assert_eq!(v["note"], "Moose", "the note is not touched by placing");
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([changed]), json!([])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list[1]["cfi"], "epubcfi(/6/20!/4/12,/1:0,/1:6)");

    // Removed on the device: removed here.
    from_device(&app, Method::PATCH, &uri, DEVICE, Some(patch(json!([]), json!(["ffc65723-4fd2-425a-961a-f8b37f270c34"])))).await;
    let (_, list) = send(&app, Method::GET, &format!("/api/books/{book}/annotations"), Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["text"], "Smooth");

    // Not taken in: a device nobody has synced with, and a book that is not the user's.
    let (status, _) = from_device(&app, Method::PATCH, &uri, "unknown-device", Some(patch(json!([highlight]), json!([])))).await;
    assert_eq!(status, StatusCode::OK, "answered kindly all the same");
    let (status, _) = from_device(&app, Method::PATCH, &format!("/api/v3/content/{bobs_content}/annotations"), DEVICE, Some(patch(json!([highlight]), json!([])))).await;
    assert_eq!(status, StatusCode::OK);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations").fetch_one(&db).await.unwrap();
    assert_eq!(total, 1);

    // Under the token too, for a device that keeps the path.
    let (status, v) = from_device(&app, Method::PATCH, &format!("/api/kobo/{token}/api/v3/content/{content}/annotations"), DEVICE, Some(patch(json!([highlight]), json!([])))).await;
    assert_eq!((status, v), (StatusCode::OK, none));
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations").fetch_one(&db).await.unwrap();
    assert_eq!(total, 2);
}
