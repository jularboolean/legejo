//! Mail about followed shelves, and the owner's log of a shelf.

use crate::kindle_tests::mail_sink_many;
use crate::shelfevents::{Event, Kind};
use crate::testutil::{add_book, add_user, send, test_app};
use axum::http::{Method, StatusCode};
use serde_json::json;

async fn shared_shelf(db: &sqlx::AnyPool, owner: i64, name: &str, visibility: &str) -> i64 {
    sqlx::query_scalar("INSERT INTO shelves (owner_id, name, visibility) VALUES (?, ?, ?) RETURNING id")
        .bind(owner)
        .bind(name)
        .bind(visibility)
        .fetch_one(db)
        .await
        .unwrap()
}

async fn put_on(db: &sqlx::AnyPool, shelf: i64, book: i64, added_at: &str) {
    sqlx::query("INSERT INTO shelf_books (shelf_id, book_id, added_at) VALUES (?, ?, ?)").bind(shelf).bind(book).bind(added_at).execute(db).await.unwrap();
}

#[tokio::test]
async fn a_follower_is_mailed_once_a_day_about_what_is_new() {
    let (_, db, state) = test_app().await;
    let (port, sink) = mail_sink_many(1).await;
    state.live.set_mail(Some(crate::mail::MailConfig::local(port, "Legejo <books@legejo.test>")));
    let app = crate::router(state.clone());
    let (alice, _) = add_user(&db, "alice").await;
    let (bob, b) = add_user(&db, "bob").await;
    let (carol, c) = add_user(&db, "carol").await;
    let (_, d) = add_user(&db, "dave").await;
    sqlx::query("UPDATE users SET email = 'bob@legejo.test' WHERE id = ?").bind(bob).execute(&db).await.unwrap();
    sqlx::query("UPDATE users SET email = 'carol@legejo.test' WHERE id = ?").bind(carol).execute(&db).await.unwrap();
    let shelf = shared_shelf(&db, alice, "Deckare & annat", "instance").await;
    let hidden = shared_shelf(&db, alice, "Privat", "private").await;
    for cookie in [&b, &c, &d] {
        let (status, v) = send(&app, Method::PUT, &format!("/api/public/shelves/{shelf}/follow"), Some(cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
    }
    // They have followed it since yesterday.
    sqlx::query("UPDATE shelf_follows SET followed_at = ?, seen_at = ?").bind(crate::db::ts_in_days(-1)).bind(crate::db::ts_in_days(-1)).execute(&db).await.unwrap();
    // Carol turns the mail off; dave has no address.
    let (_, account) = send(&app, Method::GET, "/api/account", Some(&c), None).await;
    assert_eq!(account["follow_mail"], true, "on by default");
    let (status, account) = send(&app, Method::PUT, "/api/account/follow-mail", Some(&c), Some(json!({ "enabled": false }))).await;
    assert_eq!((status, account["follow_mail"].as_bool()), (StatusCode::OK, Some(false)));

    // A book just put on the shelf waits for more to join it.
    let (roseanna, _) = add_book(&db, alice, "Roseanna").await;
    put_on(&db, shelf, roseanna, &crate::db::now_ts()).await;
    assert_eq!(crate::followmail::round(&state).await.unwrap(), 0);

    // An hour on, it is mailed about with the books that came after it;
    // nothing about a shelf bob cannot see.
    let earlier = crate::db::ts_in_hours(-2);
    sqlx::query("UPDATE shelf_books SET added_at = ? WHERE book_id = ?").bind(&earlier).bind(roseanna).execute(&db).await.unwrap();
    let (balkong, _) = add_book(&db, alice, "Mannen på <balkongen>").await;
    sqlx::query("UPDATE books SET author = 'Sjöwall & Wahlöö' WHERE id = ?").bind(balkong).execute(&db).await.unwrap();
    put_on(&db, shelf, balkong, &crate::db::now_ts()).await;
    let (secret, _) = add_book(&db, alice, "Hemligt").await;
    put_on(&db, hidden, secret, &earlier).await;
    assert_eq!(crate::followmail::round(&state).await.unwrap(), 1);
    let mails = sink.await.unwrap();
    let (to, message) = &mails[0];
    assert_eq!(to, "bob@legejo.test");
    assert!(message.contains("Subject: =?utf-8?") || message.contains("Subject: New on"), "{message}");
    let body = message.replace("=\n", "");
    assert!(body.contains("2 new books"), "{body}");
    assert!(body.contains("Roseanna") && body.contains("balkongen"), "{body}");
    assert!(!body.contains("Hemligt"), "{body}");
    assert!(body.contains("account page"), "says where to turn it off: {body}");

    // Nothing again within the day, even with something new.
    let (sista, _) = add_book(&db, alice, "Polis, polis").await;
    put_on(&db, shelf, sista, &earlier).await;
    assert_eq!(crate::followmail::round(&state).await.unwrap(), 0);
    // A day on (the first mail went out four hours after the first two
    // books), only what came since that mail.
    let (port, sink) = mail_sink_many(1).await;
    state.live.set_mail(Some(crate::mail::MailConfig::local(port, "Legejo <books@legejo.test>")));
    sqlx::query("UPDATE shelf_books SET added_at = ? WHERE book_id IN (?, ?)").bind(crate::db::ts_in_hours(-5)).bind(roseanna).bind(balkong).execute(&db).await.unwrap();
    sqlx::query("UPDATE shelf_follows SET mailed_at = ? WHERE user_id = ?").bind(crate::db::ts_in_hours(-4)).bind(bob).execute(&db).await.unwrap();
    sqlx::query("UPDATE users SET follow_mailed_at = ? WHERE id = ?").bind(crate::db::ts_in_days(-2)).bind(bob).execute(&db).await.unwrap();
    assert_eq!(crate::followmail::round(&state).await.unwrap(), 1);
    let body = sink.await.unwrap()[0].1.replace("=\n", "");
    assert!(body.contains("1 new book") && body.contains("Polis, polis"), "{body}");
    assert!(!body.contains("Roseanna"), "already told: {body}");

    // Without mail set up, nothing is tried.
    state.live.set_mail(None);
    assert_eq!(crate::followmail::round(&state).await.unwrap(), 0);
}

#[tokio::test]
async fn the_owner_sees_what_happened_on_a_shelf() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(alice).execute(&db).await.unwrap();
    let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&a), Some(json!({ "libris_enabled": false, "registration_enabled": false, "shelf_links_enabled": true }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let manual = crate::welcome::shelve(&state, alice).await.unwrap();
    let shelf = shared_shelf(&db, alice, "Till vänner", "instance").await;
    put_on(&db, shelf, manual, &crate::db::now_ts()).await;
    let events = format!("/api/shelves/{shelf}/events");

    // Bob follows the shelf and takes a copy of its book.
    send(&app, Method::PUT, &format!("/api/public/shelves/{shelf}/follow"), Some(&b), None).await;
    let (status, v) = send(&app, Method::POST, &format!("/api/public/books/{manual}/import"), Some(&b), None).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");

    // Cecilia's instance downloads it through her link, twice in a row.
    let (_, link) = send(&app, Method::POST, &format!("/api/shelves/{shelf}/links"), Some(&a), Some(json!({ "name": "Cecilia" }))).await;
    let token = link["token"].as_str().unwrap();
    for _ in 0..2 {
        let (status, _) = send(&app, Method::GET, &format!("/api/opds/shared/{token}/books/{manual}/file"), None, None).await;
        assert_eq!(status, StatusCode::OK);
    }
    send(&app, Method::DELETE, &format!("/api/public/shelves/{shelf}/follow"), Some(&b), None).await;

    // As the federation records them: a follower elsewhere, a download.
    sqlx::query("UPDATE shelves SET visibility = 'federated' WHERE id = ?").bind(shelf).execute(&db).await.unwrap();
    let event = Event { kind: Kind::FedFollowed, actor_id: None, actor_name: Some("@ada@books.example"), book: None, detail: None };
    crate::shelfevents::record(&state, shelf, event).await;
    let event = Event { kind: Kind::FedDownload, actor_id: None, actor_name: None, book: Some((manual, "The Legejo Cookbook")), detail: None };
    crate::shelfevents::record_on_shelves_of(&state, manual, "s.visibility = 'federated'", event).await;

    let (status, page) = send(&app, Method::GET, &events, Some(&a), None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let kinds: Vec<&str> = page["entries"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["fed_download", "fed_followed", "unfollowed", "link_download", "imported", "followed"], "newest first, the repeated download once");
    let entries = page["entries"].as_array().unwrap();
    assert_eq!((entries[4]["actor_name"].as_str(), entries[4]["book_title"].as_str()), (Some("bob"), Some("The Legejo Cookbook")));
    assert_eq!(entries[3]["detail"], "Cecilia");
    assert_eq!(entries[1]["actor_name"], "@ada@books.example");
    assert!(page["next"].is_null());
    let (_, first) = send(&app, Method::GET, &format!("{events}?limit=2"), Some(&a), None).await;
    let (_, rest) = send(&app, Method::GET, &format!("{events}?limit=10&before={}", first["next"]), Some(&a), None).await;
    assert_eq!(rest["entries"].as_array().unwrap().len(), 4);

    // Only the owner sees it.
    let (status, _) = send(&app, Method::GET, &events, Some(&b), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
