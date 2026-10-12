//! A book marked as read or unread by hand, what changed since a version,
//! and the Kobo devices an account has seen.

#![cfg(test)]

use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn kobo_token(app: &Router, cookie: &str) -> String {
    let (status, v) = send(app, Method::POST, "/api/account/kobo-token", Some(cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    v["kobo_token"].as_str().unwrap().to_string()
}

async fn as_device(app: &Router, method: Method, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-kobo-deviceid", "d9f26b8441cbf1519a6cf89b1a2fbd864dbfbd35e1db395178965909c06683a7")
        .header("x-kobo-devicemodel", "Kobo Clara 2E")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_book_is_read_when_marked_so_and_unread_again_everywhere_when_marked_unread() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    // A uuid of the kind the device writes, with dashes.
    let uuid = "41069f5ea598afe0ec43e1eec0e97766";
    let book: i64 = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, 'Röda rummet', 1) RETURNING id")
        .bind(uuid)
        .bind(alice)
        .fetch_one(&db)
        .await
        .unwrap();
    let token = kobo_token(&app, &a).await;
    let dashed = format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32]);

    // Half read in the web reader, and on the Kobo.
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2/1:0)", "percent": 0.5 }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    sqlx::query("INSERT INTO kosync_progress (user_id, document, book_id, progress, percentage, updated_at) VALUES (?, 'doc', ?, '/body/p[3]', 0.4, '2026-10-01T00:00:00.000Z')")
        .bind(alice)
        .bind(book)
        .execute(&db)
        .await
        .unwrap();
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["progress_percent"], 0.5);
    assert!(v["read_at"].is_null());

    // Marked as read: finished whatever the position says, and the Kobo is told.
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&b), Some(json!({ "read": true }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "someone else's book");
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&a), Some(json!({ "read": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert!(v["read_at"].is_string(), "{v}");
    assert_eq!(v["progress_percent"], 1.0, "the Kobo state says finished, and it is the newest");
    let (_, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/library/{dashed}/state"), None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "Finished", "{v}");
    let finished_at = v[0]["LastModified"].as_str().unwrap().to_string();

    // Marked as unread: the place is forgotten here, in KOReader's row, and the Kobo gets a state dated now.
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&a), Some(json!({ "read": false }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert!(v["read_at"].is_null());
    assert!(v.get("progress_percent").is_none(), "{v}");
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{book}/progress"), Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, p) = send(&app, Method::GET, &format!("/api/books/{book}/progress"), Some(&a), None).await;
    assert!(p["cfi"].is_null() && p["percent"].is_null(), "{p}");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kosync_progress WHERE book_id = ?").bind(book).fetch_one(&db).await.unwrap();
    assert_eq!(rows, 0);
    let (_, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/library/{dashed}/state"), None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "ReadyToRead");
    assert_eq!(v[0]["StatusInfo"]["TimesStartedReading"], 0);
    assert!(v[0]["LastModified"].as_str().unwrap() >= finished_at.as_str(), "dated now, so the device takes it");
    assert!(v[0]["CurrentBookmark"].get("ProgressPercent").is_none());

    // The library lists the book as unread again.
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&a), None).await;
    let mine = list.as_array().unwrap().iter().find(|x| x["id"] == book).unwrap();
    assert!(mine.get("progress_percent").is_none() && mine["read_at"].is_null());

    // Without a Kobo token, no Kobo state is made up.
    let (_, c) = add_user(&db, "cecilia").await;
    let (cecilias, _) = add_book(&db, 3, "Hemsöborna").await;
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{cecilias}/read"), Some(&c), Some(json!({ "read": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let states: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kobo_reading_state WHERE book_id = ?").bind(cecilias).fetch_one(&db).await.unwrap();
    assert_eq!(states, 0);
}

#[tokio::test]
async fn what_changed_since_a_version_is_served_from_the_binary() {
    let (app, db, _) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let (status, _) = send(&app, Method::GET, "/api/changelog?since=1.0.0", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, v) = send(&app, Method::GET, "/api/changelog?since=1.0.0", Some(&a), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    let sections = v["sections"].as_array().unwrap();
    assert!(!sections.is_empty());
    assert_eq!(sections[0]["version"], env!("CARGO_PKG_VERSION"));
    assert!(sections[0]["body"].as_str().unwrap().contains("- "));
    let (_, v) = send(&app, Method::GET, &format!("/api/changelog?since={}", env!("CARGO_PKG_VERSION")), Some(&a), None).await;
    assert_eq!(v["sections"].as_array().unwrap().len(), 0, "nothing newer than what runs");
    let (_, v) = send(&app, Method::GET, "/api/changelog", Some(&a), None).await;
    assert_eq!(v["sections"].as_array().unwrap().len(), 1, "the running version's own");
}

#[tokio::test]
async fn the_account_shows_the_kobo_devices_seen_and_how_the_last_sync_went() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    add_book(&db, alice, "Röda rummet").await;
    let (_, v) = send(&app, Method::GET, "/api/account/kobo/devices", Some(&a), None).await;
    assert_eq!(v["devices"].as_array().unwrap().len(), 0);
    assert!(v["last_sync"].is_null());

    let token = kobo_token(&app, &a).await;
    let (status, _) = as_device(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = as_device(&app, Method::GET, &format!("/api/kobo/{token}/v1/library/sync")).await;
    assert_eq!(status, StatusCode::OK);

    let (_, v) = send(&app, Method::GET, "/api/account/kobo/devices", Some(&a), None).await;
    assert_eq!(v["devices"][0]["model"], "Kobo Clara 2E", "{v}");
    assert!(v["devices"][0]["last_seen_at"].is_string());
    assert_eq!(v["last_sync"]["ok"], true);
    assert_eq!(v["last_sync"]["new"], 1);
    assert_eq!(v["syncs_last_30_days"], 1);
    // Bob sees nothing of Alice's.
    let (_, v) = send(&app, Method::GET, "/api/account/kobo/devices", Some(&b), None).await;
    assert_eq!(v["devices"].as_array().unwrap().len(), 0);
}

#[test]
fn only_a_page_on_this_site_is_gone_back_to_after_login() {
    use crate::oidc::safe_next;
    assert_eq!(safe_next("/books/12").as_deref(), Some("/books/12"));
    assert_eq!(safe_next("/shelves/3?view=list").as_deref(), Some("/shelves/3?view=list"));
    assert_eq!(safe_next("//evil.example/x"), None);
    assert_eq!(safe_next("https://evil.example/x"), None);
    assert_eq!(safe_next("/api/auth/logout"), None);
    assert_eq!(safe_next("books"), None);
    assert_eq!(safe_next("/a\nb"), None);
    assert_eq!(safe_next(" /books/12 ").as_deref(), Some("/books/12"));
}
