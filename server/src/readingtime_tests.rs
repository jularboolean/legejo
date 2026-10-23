//! How long a book takes to read: counted by the worker, at the common pace
//! until the reader's Kobo has timed books they finished, then at theirs.

use crate::library_tests::upload;
use crate::mcp_tests::novel;
use crate::readingtime::{self, COMMON_PACE};
use crate::testutil::{add_book, add_user, send, test_app};
use axum::http::Method;
use serde_json::json;

#[tokio::test]
async fn a_book_reads_at_the_common_pace_until_the_kobo_has_timed_the_reader() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let up = upload(&app, &a, &[("havet.epub", novel("Havet"))], "").await;
    let book = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));

    // Not counted yet: no estimate.
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert!(v["reading_time"].is_null(), "{v}");

    assert_eq!(readingtime::count_pending(&state, 10).await.unwrap(), 1);
    assert_eq!(readingtime::count_pending(&state, 10).await.unwrap(), 0, "counted once");
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    let chars = v["reading_time"]["chars"].as_i64().unwrap_or_else(|| panic!("{v}"));
    assert!(chars > 100, "{v}");
    assert_eq!(v["reading_time"]["pace"], COMMON_PACE);
    assert_eq!(v["reading_time"]["paced_by"], 0);

    // A finished book the Kobo timed: 360 000 characters in 300 minutes.
    let (timed, _) = add_book(&db, alice, "Röda rummet").await;
    sqlx::query("UPDATE books SET text_chars = 360000, text_chars_sha256 = '', read_at = $1 WHERE id = $2")
        .bind(crate::db::now_ts())
        .bind(timed)
        .execute(&db)
        .await
        .unwrap();
    let kobo = json!({ "Statistics": { "SpentReadingMinutes": 300 } }).to_string();
    sqlx::query("INSERT INTO kobo_reading_state (user_id, book_id, state, updated_at) VALUES ($1, $2, $3, $4)")
        .bind(alice)
        .bind(timed)
        .bind(kobo)
        .bind(crate::db::now_ts())
        .execute(&db)
        .await
        .unwrap();
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["reading_time"]["pace"], 1200, "{v}");
    assert_eq!(v["reading_time"]["paced_by"], 1);

    // Another reader's pace is their own.
    let (_, b) = add_user(&db, "bob").await;
    let up = upload(&app, &b, &[("bobs.epub", novel("Bobs bok"))], "").await;
    let bobs = up["added"][0]["id"].as_i64().unwrap();
    readingtime::count_pending(&state, 10).await.unwrap();
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{bobs}"), Some(&b), None).await;
    assert_eq!(v["reading_time"]["paced_by"], 0, "{v}");
}

#[tokio::test]
async fn the_web_readers_time_adds_up_and_paces_the_reader() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let (book, _) = add_book(&db, alice, "Gösta Berlings saga").await;
    let url = format!("/api/books/{book}/reading-time");

    // Two devices, ten minutes each.
    for _ in 0..2 {
        let (status, v) = send(&app, Method::POST, &url, Some(&a), Some(json!({ "seconds": 600 }))).await;
        assert_eq!(status, axum::http::StatusCode::NO_CONTENT, "{v}");
    }
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["reader_minutes"], 20, "{v}");

    // Nothing, too much, and another reader's book.
    for seconds in [0, -5, 6 * 3600 + 1] {
        let (status, _) = send(&app, Method::POST, &url, Some(&a), Some(json!({ "seconds": seconds }))).await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY, "{seconds}");
    }
    let (status, _) = send(&app, Method::POST, &url, Some(&b), Some(json!({ "seconds": 60 }))).await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);

    // Finished, and timed only in the web reader: 1 h 40 min more, for
    // 150 000 characters in two hours in all.
    send(&app, Method::POST, &url, Some(&a), Some(json!({ "seconds": 6000 }))).await;
    sqlx::query("UPDATE books SET text_chars = 150000, text_chars_sha256 = '', read_at = $1 WHERE id = $2")
        .bind(crate::db::now_ts())
        .bind(book)
        .execute(&db)
        .await
        .unwrap();
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["reader_minutes"], 120);
    assert_eq!(v["reading_time"]["pace"], 1250, "{v}");
    assert_eq!(v["reading_time"]["paced_by"], 1);
}
