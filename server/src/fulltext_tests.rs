//! Search inside the books: off until an admin turns it on, the worker
//! reading the books, passages around the matches, and the text dropped
//! again when the search is turned off.

use crate::fulltext;
use crate::library_tests::upload;
use crate::mcp_tests::novel;
use crate::testutil::{add_user, send, test_app};
use crate::AppState;
use std::sync::Arc;
use axum::http::{Method, StatusCode};
use axum::Router;
use serde_json::json;

/// An app with LEGEJO_FULLTEXT=true, or (`on` false) the same database
/// started again without it.
fn with_fulltext(state: &AppState, on: bool) -> (Router, AppState) {
    let mut state = state.clone();
    let mut settings = (*state.settings).clone();
    settings.fulltext = Some(on);
    state.settings = Arc::new(settings);
    (crate::router(state.clone()), state)
}

async fn test_app_on() -> (Router, sqlx::AnyPool, AppState) {
    let (_, db, state) = test_app().await;
    let (app, state) = with_fulltext(&state, true);
    (app, db, state)
}

#[tokio::test]
async fn off_unless_the_operator_turns_it_on() {
    let (app, db, _) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let (status, _) = send(&app, Method::GET, "/api/search/text?q=storm", Some(&a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["fulltext"], false);
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(&db).await.unwrap();
    let (_, settings) = send(&app, Method::GET, "/api/admin/settings", Some(&a), None).await;
    assert!(settings.get("fulltext").is_none(), "{settings}");
}

#[tokio::test]
async fn finds_words_inside_the_books_with_where_they_are() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    upload(&app, &a, &[("havet.epub", novel("Havet"))], "").await;
    upload(&app, &b, &[("bobs.epub", novel("Bobs bok"))], "").await;
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["fulltext"], true);

    // Not read yet: nothing found, and the search says why.
    let (status, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&a), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["pending"], 1);
    assert_eq!(v["books"].as_array().unwrap().len(), 0);

    assert_eq!(fulltext::index_pending(&state, 10).await.unwrap().len(), 2);
    assert!(fulltext::index_pending(&state, 10).await.unwrap().is_empty(), "a book is read once");
    // The admin page tells how far it has come: two books, one text.
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(&db).await.unwrap();
    let (_, settings) = send(&app, Method::GET, "/api/admin/settings", Some(&a), None).await;
    let status = &settings["fulltext"];
    assert_eq!((status["books"].as_i64(), status["pending"].as_i64(), status["texts"].as_i64()), (Some(2), Some(0), Some(1)), "{settings}");
    assert!(status["bytes"].as_i64().unwrap() > 100);

    // A prefix of a word, in any case; only the caller's own books.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=LIGHTHO", Some(&a), None).await;
    assert_eq!(v["pending"], 0);
    let books = v["books"].as_array().unwrap();
    assert_eq!(books.len(), 1, "{v}");
    assert_eq!(books[0]["book"]["title"], "Havet");
    let passage = &books[0]["passages"][0];
    assert_eq!(passage["title"], "The Harbour");
    assert!(passage["percent"].as_f64().unwrap() > 50.0, "{passage}");
    let text = passage["text"].as_str().unwrap();
    assert!(text.contains("\u{E000}lighthouse\u{E001}") || text.contains("\u{E000}Lighthouse\u{E001}"), "{text}");

    // Every word must be there.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=storm+lighthouse", Some(&a), None).await;
    assert_eq!(v["books"].as_array().unwrap().len(), 0, "{v}");
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=storm+sea", Some(&a), None).await;
    assert_eq!(v["books"][0]["passages"][0]["title"], "The Storm");

    // Query syntax is only text.
    let (status, _) = send(&app, Method::GET, "/api/search/text?q=%22storm%22+OR+NEAR(", Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
}

async fn parts(db: &sqlx::AnyPool) -> (i64, i64) {
    sqlx::query_as("SELECT COUNT(*), COALESCE(MAX(id), 0) FROM book_text").fetch_one(db).await.unwrap()
}

#[tokio::test]
async fn a_changed_file_with_the_same_text_writes_nothing_and_turning_off_drops_the_text() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    upload(&app, &a, &[("havet.epub", novel("Havet"))], "").await;
    let done = fulltext::index_pending(&state, 10).await.unwrap();
    assert_eq!(done[0].1, fulltext::Indexed::Stored);
    let before = parts(&db).await;
    assert!(before.0 > 0);

    // A new title is written into the file: read again, the same text.
    let id = done[0].0;
    let old_sha: String = sqlx::query_scalar("SELECT file_sha256 FROM books WHERE id = ?").bind(id).fetch_one(&db).await.unwrap();
    let (status, v) = send(&app, Method::PUT, &format!("/api/books/{id}"), Some(&a), Some(json!({ "title": "Havet, ny titel" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let new_sha: String = sqlx::query_scalar("SELECT file_sha256 FROM books WHERE id = ?").bind(id).fetch_one(&db).await.unwrap();
    assert_ne!(old_sha, new_sha, "the title is written into the file");
    assert_eq!(fulltext::index_pending(&state, 10).await.unwrap(), vec![(id, fulltext::Indexed::SameText)]);
    assert_eq!(parts(&db).await, before, "no part written or rewritten");

    // Started again without LEGEJO_FULLTEXT: the search is gone, and the
    // worker drops the text.
    let (app, state) = with_fulltext(&state, false);
    let (status, _) = send(&app, Method::GET, "/api/search/text?q=storm", Some(&a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    fulltext::clear(&state).await.unwrap();
    assert_eq!(parts(&db).await.0, 0);
    let marked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE text_sha256 IS NOT NULL OR text_hash IS NOT NULL").fetch_one(&db).await.unwrap();
    assert_eq!(marked, 0);
}

#[tokio::test]
async fn an_imported_copy_shares_the_text_of_its_original() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    let (bob, b) = add_user(&db, "bob").await;
    upload(&app, &b, &[("bobs.epub", novel("Bobs bok"))], "").await;
    fulltext::index_pending(&state, 10).await.unwrap();
    let before = parts(&db).await;
    let original: i64 = sqlx::query_scalar("SELECT id FROM books").fetch_one(&db).await.unwrap();
    let shelf: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name, visibility) VALUES (?, 'Alla', 'instance') RETURNING id")
        .bind(bob)
        .fetch_one(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)").bind(shelf).bind(original).execute(&db).await.unwrap();

    let (status, v) = send(&app, Method::POST, &format!("/api/public/books/{original}/import"), Some(&a), None).await;
    assert!(status.is_success(), "{status} {v}");
    // Searchable at once, before the worker has come by.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&a), None).await;
    assert_eq!((v["pending"].as_i64(), v["books"].as_array().unwrap().len()), (Some(0), 1), "{v}");
    assert!(fulltext::index_pending(&state, 10).await.unwrap().is_empty(), "nothing left for the worker");
    assert_eq!(parts(&db).await, before, "the copy takes no room of its own");
    let copy: i64 = sqlx::query_scalar("SELECT MAX(id) FROM books").fetch_one(&db).await.unwrap();

    // Found as Alice's own book now, not as a shared one.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&a), None).await;
    assert_eq!(v["books"][0]["book"]["id"], copy, "{v}");
    assert!(v["books"][0]["passages"][0]["text"].as_str().unwrap().contains('\u{E000}'));
    assert_eq!(v["shared"].as_array().unwrap().len(), 0, "{v}");
    // Bob still finds his, with its passages too.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&b), None).await;
    assert_eq!(v["books"][0]["passages"].as_array().unwrap().len(), 1, "{v}");

    // The text stays while one of them has it, and goes with the last.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{original}"), Some(&b), None).await;
    assert!(status.is_success());
    assert_eq!(fulltext::drop_orphans(&state).await.unwrap(), 0);
    assert_eq!(parts(&db).await, before);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{copy}"), Some(&a), None).await;
    assert!(status.is_success());
    assert_eq!(fulltext::drop_orphans(&state).await.unwrap(), before.0 as u64);
    assert_eq!(parts(&db).await.0, 0);
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=storm", Some(&a), None).await;
    assert_eq!(v["books"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_phrase_must_stand_in_a_row() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    upload(&app, &a, &[("havet.epub", novel("Havet"))], "").await;
    fulltext::index_pending(&state, 10).await.unwrap();

    let count = |v: &serde_json::Value| v["books"].as_array().unwrap().len();
    // "The lighthouse keeper saw the ship at dawn."
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=%22keeper+saw%22", Some(&a), None).await;
    assert_eq!(count(&v), 1, "{v}");
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=%22saw+keeper%22", Some(&a), None).await;
    assert_eq!(count(&v), 0, "words out of order: {v}");
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=%22keeper+the+ship%22", Some(&a), None).await;
    assert_eq!(count(&v), 0, "words not next to each other: {v}");
    // A phrase and a loose word together.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=%22ship+at+dawn%22+light", Some(&a), None).await;
    assert_eq!(count(&v), 1, "{v}");
    // A phrase matches whole words; a loose word is a prefix.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=%22lighthou%22", Some(&a), None).await;
    assert_eq!(count(&v), 0, "{v}");
}

#[tokio::test]
async fn books_on_shelves_shared_with_the_caller_are_searched_too() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    let (bob, b) = add_user(&db, "bob").await;
    let (_, c) = add_user(&db, "carol").await;
    upload(&app, &b, &[("bobs.epub", novel("Bobs bok"))], "").await;
    upload(&app, &b, &[("hemlig.epub", novel("Hemlig"))], "").await;
    fulltext::index_pending(&state, 10).await.unwrap();
    let book = |title: &str| {
        let db = db.clone();
        let title = title.to_string();
        async move { sqlx::query_scalar::<_, i64>("SELECT id FROM books WHERE title = ?").bind(title).fetch_one(&db).await.unwrap() }
    };
    let (open, secret) = (book("Bobs bok").await, book("Hemlig").await);

    // Bob shares one shelf with Alice only; the other book is on no shelf.
    let shelf: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name, restricted) VALUES (?, 'Till Alice', 1) RETURNING id")
        .bind(bob)
        .fetch_one(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)").bind(shelf).bind(open).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO shelf_members (shelf_id, user_id) SELECT ?, id FROM users WHERE username = 'alice'")
        .bind(shelf)
        .execute(&db)
        .await
        .unwrap();

    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&a), None).await;
    assert_eq!(v["books"].as_array().unwrap().len(), 0, "{v}");
    let shared = v["shared"].as_array().unwrap();
    assert_eq!(shared.len(), 1, "{v}");
    assert_eq!(shared[0]["book"]["id"], open);
    assert_eq!(shared[0]["book"]["owner"], "bob");
    assert_eq!(shared[0]["book"]["shelf_name"], "Till Alice");
    assert!(shared[0]["passages"][0]["text"].as_str().unwrap().contains('\u{E000}'));
    assert_ne!(shared[0]["book"]["id"], secret);

    // Not shared with Carol; Bob's own books are his, not shared ones.
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&c), None).await;
    assert_eq!(v["shared"].as_array().unwrap().len(), 0, "{v}");
    let (_, v) = send(&app, Method::GET, "/api/search/text?q=lighthouse", Some(&b), None).await;
    assert_eq!((v["books"].as_array().unwrap().len(), v["shared"].as_array().unwrap().len()), (2, 0), "{v}");

}

#[tokio::test]
async fn esperanto_typed_in_the_x_system_finds_the_printed_words() {
    let (app, db, state) = test_app_on().await;
    let (_, a) = add_user(&db, "alice").await;
    upload(&app, &a, &[("cxambro.epub", novel("La ĉambro de ŝanĝoj"))], "").await;
    fulltext::index_pending(&state, 10).await.unwrap();
    // The title, in the library and in the search.
    for q in ["%C4%89ambro", "cxambro", "Cxambro+sxangxoj"] {
        let (status, v) = send(&app, Method::GET, &format!("/api/books?q={q}"), Some(&a), None).await;
        assert_eq!(status, StatusCode::OK, "{q}: {v}");
        assert_eq!(v.as_array().unwrap().len(), 1, "{q}: {v}");
        let (_, v) = send(&app, Method::GET, &format!("/api/search?q={q}"), Some(&a), None).await;
        assert_eq!(v["mine"].as_array().unwrap().len(), 1, "{q}: {v}");
    }
    // The text: a query with x-system words is a valid query, single or in a phrase.
    for q in ["cxambro", "%22storm+cxe%22", "lighthouse"] {
        let (status, v) = send(&app, Method::GET, &format!("/api/search/text?q={q}"), Some(&a), None).await;
        assert_eq!(status, StatusCode::OK, "{q}: {v}");
    }
    let expr = fulltext::text_expr(crate::db::Backend::Sqlite, "cxambro \"sxangxo kaj\"").unwrap();
    assert_eq!(expr, r#"("cxambro"* OR "ĉambro"*) AND ("sxangxo kaj" OR "ŝanĝo kaj")"#);
    let expr = fulltext::text_expr(crate::db::Backend::Postgres, "cxambro \"sxangxo kaj\"").unwrap();
    assert_eq!(expr, "(cxambro:* | ĉambro:*) & ((sxangxo <-> kaj) | (ŝanĝo <-> kaj))");
}
