//! API tests for license facts and shelf visibility.

use crate::testutil::{add_book, add_user, send, test_app};
use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

fn book_body(license: Value) -> Value {
    json!({ "title": "Röda rummet", "license": license })
}

#[tokio::test]
async fn licence_facts_are_validated_and_stored() {
    let (app, db, _) = test_app().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let (book, _) = add_book(&db, alice, "Röda rummet").await;
    let uri = format!("/api/books/{book}");

    // A fresh book has no license and cannot federate.
    let (status, v) = send(&app, Method::GET, &uri, Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["license"], Value::Null);
    assert_eq!(v["cover_is_free"], false);
    assert_eq!(v["federable"], json!({ "ok": false, "code": "unknown_license" }));

    // Free text, a bad source and an impossible year are refused.
    for bad in [
        json!({ "license": "gpl" }),
        json!({ "license": "cc-by", "source_url": "runeberg.org" }),
        json!({ "license": "pd", "author_death_year": 3000 }),
    ] {
        let (status, _) = send(&app, Method::PUT, &uri, Some(&cookie), Some(book_body(bad.clone()))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }

    // Public domain with a recent death year: stored, but not federable.
    let (status, v) = send(
        &app,
        Method::PUT,
        &uri,
        Some(&cookie),
        Some(book_body(json!({ "license": "pd", "source_url": "https://runeberg.org/rodarummet/", "author_death_year": 1960 }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["license"], "pd");
    assert_eq!(v["author_death_year"], 1960);
    assert_eq!(v["federable"]["code"], "still_protected");
    assert_eq!(v["federable"]["free_from"], 2031);

    // Strindberg's real year: federable, cover confirmed free.
    let (_, v) = send(
        &app,
        Method::PUT,
        &uri,
        Some(&cookie),
        Some(book_body(json!({ "license": "pd", "source_url": "https://runeberg.org/rodarummet/", "author_death_year": 1912, "cover_is_free": true }))),
    )
    .await;
    assert_eq!(v["federable"], json!({ "ok": true }));
    assert_eq!(v["cover_is_free"], true);

    // An update without the license key leaves the license alone.
    let (_, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "title": "Röda rummet" }))).await;
    assert_eq!(v["license"], "pd");
    assert_eq!(v["author_death_year"], 1912);

    // Clearing: null license and no year.
    let (_, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(book_body(json!({ "license": null })))).await;
    assert_eq!(v["license"], Value::Null);
    assert_eq!(v["author_death_year"], Value::Null);
    assert_eq!(v["license_source_url"], Value::Null);
}

async fn create_shelf(app: &axum::Router, cookie: &str, name: &str) -> i64 {
    let (status, v) = send(app, Method::POST, "/api/shelves", Some(cookie), Some(json!({ "name": name }))).await;
    assert_eq!(status, StatusCode::CREATED);
    v["id"].as_i64().unwrap()
}

#[tokio::test]
async fn shelf_visibility_replaces_is_public() {
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let shelf = create_shelf(&app, &cookie, "Klassiker").await;
    let uri = format!("/api/shelves/{shelf}");

    let (_, v) = send(&app, Method::GET, &uri, Some(&cookie), None).await;
    assert_eq!(v["visibility"], "private");
    assert_eq!(v["is_public"], false);

    // The new field
    let (status, v) =
        send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "name": "Klassiker", "visibility": "instance" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["visibility"], "instance");
    assert_eq!(v["is_public"], true);

    // and the old boolean, from clients that predate it.
    let (_, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "name": "Klassiker", "is_public": false }))).await;
    assert_eq!(v["visibility"], "private");
    assert_eq!(v["is_public"], false);

    // Without a federation config, federated visibility is refused.
    let (status, _) =
        send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "name": "Klassiker", "visibility": "federated" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) =
        send(&app, Method::PUT, &uri, Some(&cookie), Some(json!({ "name": "Klassiker", "visibility": "everyone" }))).await;
    assert!(status.is_client_error());
}

/// Instance visibility has no license gate. A copyrighted book on an instance
/// shelf is visible to and importable by another local user, and the copy
/// keeps the license facts and the file hash.
#[tokio::test]
async fn instance_shelves_have_no_licence_gate() {
    let (app, db, state) = test_app().await;
    let (alice, alice_cookie) = add_user(&db, "alice").await;
    let (_, carol_cookie) = add_user(&db, "carol").await;
    let (book, uuid) = add_book(&db, alice, "Skyddad").await;
    std::fs::write(state.data_dir.join("books").join(format!("{uuid}.epub")), b"epub bytes").unwrap();
    sqlx::query("UPDATE books SET license = 'copyright', file_sha256 = 'abc' WHERE id = ?")
        .bind(book)
        .execute(&db)
        .await
        .unwrap();

    let shelf = create_shelf(&app, &alice_cookie, "privat-skyddad").await;
    send(&app, Method::PUT, &format!("/api/shelves/{shelf}"), Some(&alice_cookie),
         Some(json!({ "name": "privat-skyddad", "visibility": "instance" }))).await;
    sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)")
        .bind(shelf)
        .bind(book)
        .execute(&db)
        .await
        .unwrap();

    let (status, v) = send(&app, Method::GET, "/api/public/shelves", Some(&carol_cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v.as_array().unwrap().len(), 1);

    let (status, v) = send(&app, Method::GET, "/api/public/alice/privat-skyddad", Some(&carol_cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["books"][0]["license"], "copyright");

    let (status, v) =
        send(&app, Method::POST, &format!("/api/public/books/{book}/import"), Some(&carol_cookie), None).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["license"], "copyright");
    let copy = v["id"].as_i64().unwrap();
    let hash: Option<String> = sqlx::query_scalar("SELECT file_sha256 FROM books WHERE id = ?")
        .bind(copy)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(hash.as_deref(), Some("abc"));

    // A private shelf is not visible at all.
    send(&app, Method::PUT, &format!("/api/shelves/{shelf}"), Some(&alice_cookie),
         Some(json!({ "name": "privat-skyddad", "visibility": "private" }))).await;
    let (_, v) = send(&app, Method::GET, "/api/public/shelves", Some(&carol_cookie), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn sha256_backfill_hashes_existing_files() {
    let (_, db, state) = test_app().await;
    let (alice, _) = add_user(&db, "alice").await;
    let (book, uuid) = add_book(&db, alice, "Gammal").await;
    let (missing, _) = add_book(&db, alice, "Saknar fil").await;
    std::fs::write(state.data_dir.join("books").join(format!("{uuid}.epub")), b"abc").unwrap();

    crate::books::backfill_sha256(state.clone()).await;

    let hash: Option<String> =
        sqlx::query_scalar("SELECT file_sha256 FROM books WHERE id = ?").bind(book).fetch_one(&db).await.unwrap();
    // SHA-256("abc")
    assert_eq!(hash.as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    let none: Option<String> =
        sqlx::query_scalar("SELECT file_sha256 FROM books WHERE id = ?").bind(missing).fetch_one(&db).await.unwrap();
    assert_eq!(none, None);
}

