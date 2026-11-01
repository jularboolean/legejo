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

    // Marked as unread on the device: the web's place and the mark by hand go, being older.
    send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&a), Some(json!({ "read": true }))).await;
    send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2/1:0)", "percent": 0.3 }))).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let unread = json!({ "ReadingStates": [{ "StatusInfo": { "Status": "ReadyToRead", "LastModified": crate::db::now_ts(), "TimesStartedReading": 0 },
                                              "CurrentBookmark": { "ProgressPercent": 30, "LastModified": crate::db::now_ts() } }] });
    let (status, _) = send(&app, Method::PUT, &format!("/api/kobo/{token}/v1/library/{dashed}/state"), None, Some(unread)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert!(v.get("progress_percent").is_none(), "unread, whatever bookmark the state carries: {v}");
    assert!(v["read_at"].is_null());
    // A place read in the web after the device's decision is kept.
    sqlx::query("UPDATE kobo_reading_state SET state = ? WHERE book_id = ?")
        .bind(json!({ "StatusInfo": { "Status": "ReadyToRead", "LastModified": "2020-01-01T00:00:00.000Z" } }).to_string())
        .bind(book)
        .execute(&db)
        .await
        .unwrap();
    send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2/1:0)", "percent": 0.6 }))).await;
    let old = json!({ "ReadingStates": [{ "StatusInfo": { "Status": "ReadyToRead", "LastModified": "2020-01-01T00:00:00Z" } }] });
    send(&app, Method::PUT, &format!("/api/kobo/{token}/v1/library/{dashed}/state"), None, Some(old)).await;
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["progress_percent"], 0.6, "{v}");

    // A stored "unread" from the device that is newer than the web's place counts, even
    // before the device says it again (what an upgrade finds in the database).
    send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2/1:0)", "percent": 0.7 }))).await;
    sqlx::query("UPDATE kobo_reading_state SET state = ?, updated_at = ? WHERE book_id = ?")
        .bind(json!({ "StatusInfo": { "Status": "ReadyToRead", "LastModified": crate::db::ts_in_hours(1).replace(".000Z", "Z") }, "CurrentBookmark": { "ProgressPercent": 70 } }).to_string())
        .bind(crate::db::ts_in_hours(1))
        .bind(book)
        .execute(&db)
        .await
        .unwrap();
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert!(v.get("progress_percent").is_none(), "{v}");
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&a), None).await;
    let mine = list.as_array().unwrap().iter().find(|x| x["id"] == book).unwrap();
    assert!(mine.get("progress_percent").is_none() && mine.get("last_read_at").is_none(), "{mine}");

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

/// One sync round as the device, with the sync token of the round before.
async fn sync(app: &Router, token: &str, sync_token: Option<&str>) -> (Vec<Value>, String) {
    let mut req = Request::builder().method(Method::GET).uri(format!("/api/kobo/{token}/v1/library/sync"));
    if let Some(sync_token) = sync_token {
        req = req.header("x-kobo-synctoken", sync_token);
    }
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let next = res.headers().get("x-kobo-synctoken").unwrap().to_str().unwrap().to_string();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (serde_json::from_slice::<Value>(&bytes).unwrap().as_array().cloned().unwrap_or_default(), next)
}

#[tokio::test]
async fn a_place_read_in_the_web_points_the_device_to_the_same_sentence() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let uuid = "5f1c2a9d3b4e4c6f8a7b9c0d1e2f3a4c";
    let book: i64 = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, 'Alice', 1) RETURNING id")
        .bind(uuid)
        .bind(alice)
        .fetch_one(&db)
        .await
        .unwrap();
    // The KEPUB the device reads, as kepubify leaves it in the cache.
    let dir = state.data_dir.join("kepub");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join(format!("{uuid}.kepub.epub")), crate::kobospan::sample_kepub()).await.unwrap();
    let token = kobo_token(&app, &a).await;
    let dashed = format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32]);
    let state_url = format!("/api/kobo/{token}/v1/library/{dashed}/state");
    let progress_url = format!("/api/books/{book}/progress");

    // Not opened anywhere: the device starts the book fresh.
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "ReadyToRead");

    // Read in the web as far as the Gryphon's line: the device is pointed to
    // the sentence those words begin, dated by the web's time.
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/4/1:0)", "percent": 0.42, "text": "“Why,” said the Gryphon, “you first form\n into a line" });
    let (status, _) = send(&app, Method::PUT, &progress_url, Some(&a), Some(web)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    let read_at: String = sqlx::query_scalar("SELECT updated_at FROM reading_progress WHERE book_id = ?").bind(book).fetch_one(&db).await.unwrap();
    assert_eq!(v[0]["StatusInfo"]["Status"], "Reading", "{v}");
    assert_eq!(v[0]["StatusInfo"]["TimesStartedReading"], 1);
    assert_eq!(v[0]["LastModified"], read_at);
    assert_eq!(v[0]["StatusInfo"]["LastModified"], read_at);
    let bookmark = &v[0]["CurrentBookmark"];
    assert_eq!(bookmark["LastModified"], read_at);
    assert_eq!(bookmark["ProgressPercent"], 42);
    let in_chapter = bookmark["ContentSourceProgressPercent"].as_i64().unwrap();
    assert!((1..50).contains(&in_chapter), "early in the chapter: {bookmark}");
    assert_eq!(bookmark["Location"], json!({ "Value": "kobo.5.1", "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" }));
    let placed: Option<String> = sqlx::query_scalar("SELECT kobo_json FROM reading_progress WHERE book_id = ?").bind(book).fetch_one(&db).await.unwrap();
    assert!(placed.is_some_and(|p| p.contains("kobo.5.1")), "placed once, kept for later syncs");
    let (_, p) = send(&app, Method::GET, &progress_url, Some(&a), None).await;
    assert_eq!(p["cfi"], "epubcfi(/6/4[ch1]!/4/4/1:0)", "the web keeps its own position");

    // A sync carries the place in the book's entitlement, and, read further
    // between syncs, as a changed reading state; then nothing more.
    let (items, sync_token) = sync(&app, &token, None).await;
    let new = items.iter().find(|i| i["NewEntitlement"]["BookEntitlement"]["Id"] == dashed).expect("the book is new to the device");
    assert_eq!(new["NewEntitlement"]["ReadingState"]["CurrentBookmark"]["Location"]["Value"], "kobo.5.1");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/6/1:0)", "percent": 0.9, "text": "crumbs would all wash off in the sea" });
    send(&app, Method::PUT, &progress_url, Some(&a), Some(web)).await;
    let (items, sync_token) = sync(&app, &token, Some(&sync_token)).await;
    let changed: Vec<&Value> = items.iter().filter(|i| i.get("ChangedReadingState").is_some()).collect();
    assert_eq!(changed.len(), 1, "{items:?}");
    let reading_state = &changed[0]["ChangedReadingState"]["ReadingState"];
    assert_eq!(reading_state["EntitlementId"], dashed);
    assert_eq!(reading_state["CurrentBookmark"]["Location"]["Value"], "kobo.31.1");
    assert_eq!(reading_state["CurrentBookmark"]["ProgressPercent"], 90);
    let (items, sync_token) = sync(&app, &token, Some(&sync_token)).await;
    assert!(items.iter().all(|i| i.get("ChangedReadingState").is_none()), "sent once: {items:?}");

    // The device then reads on and says so: its word is the newer one, and
    // the web's older place is not sent back over it.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let now = crate::db::now_ts().replace(".000Z", "Z");
    let device = json!({ "ReadingStates": [{
        "StatusInfo": { "Status": "Reading", "LastModified": now, "TimesStartedReading": 1 },
        "Statistics": { "LastModified": now, "SpentReadingMinutes": 7 },
        "CurrentBookmark": { "LastModified": now, "ProgressPercent": 95, "ContentSourceProgressPercent": 80,
                             "Location": { "Value": "kobo.31.4", "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" } },
    }] });
    let (status, _) = send(&app, Method::PUT, &state_url, None, Some(device)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["CurrentBookmark"]["Location"]["Value"], "kobo.31.4");
    assert_eq!(v[0]["CurrentBookmark"]["ProgressPercent"], 95);
    let (items, sync_token) = sync(&app, &token, Some(&sync_token)).await;
    let changed: Vec<&Value> = items.iter().filter(|i| i.get("ChangedReadingState").is_some()).collect();
    assert_eq!(changed.len(), 1, "the device's own state comes back to it, as before");
    assert_eq!(changed[0]["ChangedReadingState"]["ReadingState"]["CurrentBookmark"]["ProgressPercent"], 95);

    // Read again in the web, but words the KEPUB does not have: the device
    // keeps its own place, what it counted stays, and the catalog still
    // shows the web's newer position.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/6/1:0)", "percent": 0.5, "text": "words that are not in the book" });
    send(&app, Method::PUT, &progress_url, Some(&a), Some(web)).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["CurrentBookmark"]["ProgressPercent"], 95, "{v}");
    assert_eq!(v[0]["Statistics"]["SpentReadingMinutes"], 7);
    let (_, b) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(b["progress_percent"], 0.5);
    // ...and a position found keeps the device's statistics as well.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/2/1:0)", "percent": 0.1, "text": "No, indeed" });
    send(&app, Method::PUT, &progress_url, Some(&a), Some(web)).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["CurrentBookmark"]["Location"]["Value"], "kobo.4.1");
    assert_eq!(v[0]["CurrentBookmark"]["ProgressPercent"], 10);
    assert_eq!(v[0]["Statistics"]["SpentReadingMinutes"], 7);
    assert_eq!(v[0]["StatusInfo"]["TimesStartedReading"], 1);
    let (items, _) = sync(&app, &token, Some(&sync_token)).await;
    let changed: Vec<&Value> = items.iter().filter(|i| i.get("ChangedReadingState").is_some()).collect();
    assert_eq!(changed.len(), 1, "one state for the book, however many times it was read: {items:?}");

    // Started over by hand: the device hears that, not the old place.
    send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&a), Some(json!({ "read": false }))).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "ReadyToRead");
    assert!(v[0]["CurrentBookmark"].get("Location").is_none());

    // A position without text, as an older web reader sends it, points nowhere.
    send(&app, Method::PUT, &progress_url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/4[ch1]!/4/2/1:0)", "percent": 0.2 }))).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "ReadyToRead");
}

#[tokio::test]
async fn the_kobo_reading_time_is_kept_and_the_web_opens_at_the_kobo_line() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let uuid = "7a2c3b4d5e6f40718293a4b5c6d7e8f9";
    let book: i64 = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, 'Alice', 1) RETURNING id")
        .bind(uuid)
        .bind(alice)
        .fetch_one(&db)
        .await
        .unwrap();
    let dir = state.data_dir.join("kepub");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join(format!("{uuid}.kepub.epub")), crate::kobospan::sample_kepub()).await.unwrap();
    let token = kobo_token(&app, &a).await;
    let dashed = format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32]);
    let state_url = format!("/api/kobo/{token}/v1/library/{dashed}/state");
    let progress_url = format!("/api/books/{book}/progress");

    // The device reads, counts 42 minutes, and stops at the Gryphon's line.
    let now = crate::db::now_ts().replace(".000Z", "Z");
    let device = json!({ "ReadingStates": [{
        "StatusInfo": { "Status": "Reading", "LastModified": now, "TimesStartedReading": 1 },
        "Statistics": { "LastModified": now, "SpentReadingMinutes": 42, "RemainingTimeMinutes": 90 },
        "CurrentBookmark": { "LastModified": now, "ProgressPercent": 40, "ContentSourceProgressPercent": 30,
                             "Location": { "Value": "kobo.5.1", "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" } },
    }] });
    let (status, _) = send(&app, Method::PUT, &state_url, None, Some(device)).await;
    assert_eq!(status, StatusCode::OK);

    // The time is on the book, in the library and with the book.
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["kobo_minutes"], 42, "{v}");
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&a), None).await;
    let listed = list.as_array().unwrap().iter().find(|b| b["id"] == book).unwrap();
    assert_eq!(listed["kobo_minutes"], 42);

    // The web is told the words at the device's line, to open there.
    let (_, p) = send(&app, Method::GET, &progress_url, Some(&a), None).await;
    assert!(p["cfi"].is_null(), "{p}");
    assert_eq!(p["percent"], 0.4);
    assert_eq!(p["kobo_text"], "“Why,” said the Gryphon, “you first form into a line along the sea-shore—”", "{p}");
    assert_eq!(p["kobo_chapter"], "OEBPS/xhtml/ch1.xhtml");

    // A state without statistics keeps the count.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let now = crate::db::now_ts().replace(".000Z", "Z");
    let bare = json!({ "ReadingStates": [{
        "StatusInfo": { "Status": "Reading", "LastModified": now, "TimesStartedReading": 1 },
        "CurrentBookmark": { "LastModified": now, "ProgressPercent": 45,
                             "Location": { "Value": "kobo.31.2", "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" } },
    }] });
    send(&app, Method::PUT, &state_url, None, Some(bare)).await;
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["kobo_minutes"], 42);
    let (_, p) = send(&app, Method::GET, &progress_url, Some(&a), None).await;
    assert!(p["kobo_text"].as_str().unwrap().starts_with("But they have their tails"), "{p}");

    // Marked as read in the web: the device is told so, with its count kept.
    let (status, _) = send(&app, Method::PUT, &format!("/api/books/{book}/read"), Some(&a), Some(json!({ "read": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["StatusInfo"]["Status"], "Finished");
    assert_eq!(v[0]["Statistics"]["SpentReadingMinutes"], 42, "{v}");
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["kobo_minutes"], 42);

    // Read further in the web: its own place wins, and no Kobo line is sent.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/6/1:0)", "percent": 0.9, "text": "crumbs would all wash off in the sea" });
    send(&app, Method::PUT, &progress_url, Some(&a), Some(web)).await;
    let (_, p) = send(&app, Method::GET, &progress_url, Some(&a), None).await;
    assert_eq!(p["cfi"], "epubcfi(/6/4[ch1]!/4/6/1:0)");
    assert!(p.get("kobo_text").is_none(), "{p}");

    // A book without a KEPUB in the cache (the device has not downloaded it
    // from here) gives the percentage only.
    tokio::fs::remove_file(dir.join(format!("{uuid}.kepub.epub"))).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let now = crate::db::now_ts().replace(".000Z", "Z");
    let device = json!({ "ReadingStates": [{
        "StatusInfo": { "Status": "Reading", "LastModified": now, "TimesStartedReading": 2 },
        "CurrentBookmark": { "LastModified": now, "ProgressPercent": 95,
                             "Location": { "Value": "kobo.31.4", "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" } },
    }] });
    send(&app, Method::PUT, &state_url, None, Some(device)).await;
    let (_, p) = send(&app, Method::GET, &progress_url, Some(&a), None).await;
    assert_eq!(p["percent"], 0.95, "{p}");
    assert!(p.get("kobo_text").is_none(), "{p}");
}

#[tokio::test]
async fn a_device_that_only_opened_a_book_does_not_outvote_the_web() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let uuid = "5f1c2a9d3b4e4c6f8a7b9c0d1e2f3a4d";
    let book: i64 = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, 'Alice', 1) RETURNING id")
        .bind(uuid)
        .bind(alice)
        .fetch_one(&db)
        .await
        .unwrap();
    let dir = state.data_dir.join("kepub");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join(format!("{uuid}.kepub.epub")), crate::kobospan::sample_kepub()).await.unwrap();
    let token = kobo_token(&app, &a).await;
    let dashed = format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32]);
    let state_url = format!("/api/kobo/{token}/v1/library/{dashed}/state");
    let place = |value: &str, percent: i64| json!({ "ReadingStates": [{
        "StatusInfo": { "Status": "Reading" },
        "CurrentBookmark": { "ProgressPercent": percent, "Location": { "Value": value, "Type": "KoboSpan", "Source": "OEBPS/xhtml/ch1.xhtml" } },
    }] });

    // The device has been at the start; then the book is read on in the web.
    send(&app, Method::PUT, &state_url, None, Some(place("kobo.1.1", 5))).await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let web = json!({ "cfi": "epubcfi(/6/4[ch1]!/4/4/1:0)", "percent": 0.42, "text": "“Why,” said the Gryphon, “you first form\n into a line" });
    send(&app, Method::PUT, &format!("/api/books/{book}/progress"), Some(&a), Some(web)).await;

    // The device reports the place it already had: the web's still stands.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    send(&app, Method::PUT, &state_url, None, Some(place("kobo.1.1", 5))).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["CurrentBookmark"]["Location"]["Value"], "kobo.5.1", "{v}");

    // Read further on the device, and that is the latest word.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    send(&app, Method::PUT, &state_url, None, Some(place("kobo.1.3", 6))).await;
    let (_, v) = send(&app, Method::GET, &state_url, None, None).await;
    assert_eq!(v[0]["CurrentBookmark"]["Location"]["Value"], "kobo.1.3", "{v}");
}
