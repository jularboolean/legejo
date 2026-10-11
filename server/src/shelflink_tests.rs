//! A shelf shared by a private link, from the owner making the link to
//! another user following it and taking a book home.

#![cfg(test)]

use crate::testutil::{add_book, add_user, send, test_app, test_app_private};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::json;
use tower::ServiceExt;

async fn turn_on(app: &Router, db: &sqlx::AnyPool, admin_cookie: &str) {
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(db).await.unwrap();
    let body = json!({ "libris_enabled": true, "registration_enabled": false, "shelf_links_enabled": true });
    let (status, v) = send(app, Method::PUT, "/api/admin/settings", Some(admin_cookie), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["shelf_links_enabled"], true);
}

async fn shelf_with(db: &sqlx::AnyPool, owner: i64, name: &str, description: Option<&str>, books: &[i64]) -> i64 {
    let id: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name, description) VALUES (?, ?, ?) RETURNING id")
        .bind(owner)
        .bind(name)
        .bind(description)
        .fetch_one(db)
        .await
        .unwrap();
    for book in books {
        sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)").bind(id).bind(book).execute(db).await.unwrap();
    }
    id
}

/// A request without a login, as a reading app makes it.
async fn open(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let res = app.clone().oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    (status, headers, axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec())
}

#[tokio::test]
async fn off_until_an_admin_turns_it_on() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let shelf = shelf_with(&db, alice, "Sommar", None, &[]).await;
    let (status, _) = send(&app, Method::GET, &format!("/api/shelves/{shelf}/links"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::POST, &format!("/api/shelves/{shelf}/links"), Some(&a), Some(json!({ "name": "Bob" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::GET, "/api/linked-shelves", Some(&a), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = open(&app, &format!("/api/opds/shared/{}", "ab".repeat(32))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["shelf_links"], false);
}

#[tokio::test]
async fn a_link_gives_the_shelf_to_whoever_holds_it_until_it_is_taken_back() {
    let (app, db, state) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    turn_on(&app, &db, &a).await;
    let manual = crate::welcome::shelve(&state, alice).await.unwrap();
    let (elsewhere, _) = add_book(&db, alice, "Not on the shelf").await;
    sqlx::query("UPDATE books SET rating = 5 WHERE id = ?").bind(manual).execute(&db).await.unwrap();
    let shelf = shelf_with(&db, alice, "Till <vänner> & bekanta", Some("Det jag läst i år."), &[manual]).await;

    // Only the owner makes links, and a link needs a name.
    let links = format!("/api/shelves/{shelf}/links");
    let (status, _) = send(&app, Method::POST, &links, Some(&b), Some(json!({ "name": "Mig själv" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::POST, &links, Some(&a), Some(json!({ "name": "  " }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = send(&app, Method::POST, &links, Some(&a), Some(json!({ "name": "Bob", "days": 0 }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, link) = send(&app, Method::POST, &links, Some(&a), Some(json!({ "name": " Bobs  instans " }))).await;
    assert_eq!(status, StatusCode::CREATED, "{link}");
    assert_eq!(link["name"], "Bobs instans");
    assert!(link["expires_at"].is_null() && link["last_used_at"].is_null());
    let token = link["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 64);
    let (_, another) = send(&app, Method::POST, &links, Some(&a), Some(json!({ "name": "Cecilia", "days": 30 }))).await;
    assert!(another["expires_at"].as_str().unwrap() > crate::db::now_ts().as_str());
    assert_ne!(another["token"], token);

    // The feed: the shelf, what it says about itself, whose it is, and its books. No login.
    let base = format!("/api/opds/shared/{token}");
    let (status, headers, body) = open(&app, &base).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE].to_str().unwrap().contains("kind=acquisition"));
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<title>Till &lt;vänner&gt; &amp; bekanta</title>"), "{xml}");
    assert!(xml.contains("<subtitle>Det jag läst i år.</subtitle>"));
    assert!(xml.contains("<author><name>alice</name></author>"));
    assert!(xml.contains("<generator uri=\"https://github.com/jularboolean/legejo\">Legejo</generator>"));
    assert!(xml.contains("<title>The Legejo Cookbook</title>"));
    assert!(xml.contains(&format!("href=\"{base}/books/{manual}/file\" type=\"application/epub+zip\"")));
    assert!(xml.contains(&format!("href=\"{base}/books/{manual}/cover\"")));
    assert!(!xml.contains("Not on the shelf"));
    assert!(!xml.contains("Rating:"), "the owner's rating is theirs");
    assert!(!xml.contains("/api/opds/books/") && !xml.contains("/api/opds/series/"), "nothing that asks for a login");

    // The cover and the file of a book on the shelf; nothing else of the owner's.
    let (status, headers, cover) = open(&app, &format!("{base}/books/{manual}/cover")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("image/"));
    assert!(cover.len() > 1000);
    let (status, headers, file) = open(&app, &format!("{base}/books/{manual}/file")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/epub+zip");
    assert!(file.starts_with(b"PK"));
    let (status, _, _) = open(&app, &format!("{base}/books/{elsewhere}/file")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = open(&app, &format!("{base}/books/{elsewhere}/cover")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The owner sees that it has been used.
    let (_, list) = send(&app, Method::GET, &links, Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert!(list[0]["last_used_at"].is_string());
    assert!(list[1]["last_used_at"].is_null());
    let (status, _) = send(&app, Method::GET, &links, Some(&b), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A link past its last day is not there.
    sqlx::query("UPDATE shelf_links SET expires_at = '2020-01-01T00:00:00.000Z' WHERE id = ?").bind(another["id"].as_i64().unwrap()).execute(&db).await.unwrap();
    let (status, _, _) = open(&app, &format!("/api/opds/shared/{}", another["token"].as_str().unwrap())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Taken back: gone at once, for the feed and for the files. The other link is not disturbed.
    let id = link["id"].as_i64().unwrap();
    let (status, _) = send(&app, Method::DELETE, &format!("{links}/{id}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::DELETE, &format!("{links}/{id}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = open(&app, &base).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = open(&app, &format!("{base}/books/{manual}/file")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = send(&app, Method::GET, &links, Some(&a), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    // Tokens that are no tokens.
    for bad in ["short", &"zz".repeat(32), &"ab".repeat(33)] {
        let (status, _, _) = open(&app, &format!("/api/opds/shared/{bad}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn a_followed_link_is_a_shelf_among_those_shared_with_you() {
    // One instance, reached over the network at its own address, as another would be.
    let (app, db, state) = test_app_private().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let there = format!("http://{}", listener.local_addr().unwrap());
    let served = app.clone();
    tokio::spawn(async move { axum::serve(listener, served).await.unwrap() });

    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let (_, c) = add_user(&db, "cecilia").await;
    turn_on(&app, &db, &a).await;
    let manual = crate::welcome::shelve(&state, alice).await.unwrap();
    let shelf = shelf_with(&db, alice, "Handböcker", Some("Bra att ha."), &[manual]).await;
    let (_, link) = send(&app, Method::POST, &format!("/api/shelves/{shelf}/links"), Some(&a), Some(json!({ "name": "Bob" }))).await;
    let address = format!("{there}/api/opds/shared/{}", link["token"].as_str().unwrap());

    // What is not a link to a shelf is not followed, and nothing is fetched for it.
    for bad in [format!("{there}/api/opds"), "https://example.org/feed.xml".to_string(), String::new()] {
        let (status, v) = send(&app, Method::POST, "/api/linked-shelves", Some(&b), Some(json!({ "url": bad }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
        assert_eq!(v["error"], "not a shelf link");
    }
    // A link nobody made.
    let (status, v) = send(&app, Method::POST, "/api/linked-shelves", Some(&b), Some(json!({ "url": format!("{there}/api/opds/shared/{}", "ab".repeat(32)) }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{v}");
    assert_eq!(v["error"], "missing");

    // Followed: it has the shelf's name and description, and says where it is from, not its address.
    let (status, v) = send(&app, Method::POST, "/api/linked-shelves", Some(&b), Some(json!({ "url": format!(" {address}/ ") }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["title"], "Handböcker");
    assert_eq!(v["description"], "Bra att ha.");
    assert_eq!(v["host"], "127.0.0.1");
    assert!(v.get("url").is_none(), "the address is a secret: {v}");
    let id = v["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::POST, "/api/linked-shelves", Some(&b), Some(json!({ "url": address }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["id"], id);

    // It is Bob's alone.
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&c), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
    let (status, _) = send(&app, Method::GET, &format!("/api/linked-shelves/{id}"), Some(&c), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert!(v[0].get("url").is_none());

    // Opened: the books as they stand on the shelf now, and a change of name follows.
    sqlx::query("UPDATE shelves SET name = 'Handböcker och annat' WHERE id = ?").bind(shelf).execute(&db).await.unwrap();
    let (status, v) = send(&app, Method::GET, &format!("/api/linked-shelves/{id}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["shelf"]["title"], "Handböcker och annat");
    let book = &v["books"][0];
    assert_eq!(book["title"], "The Legejo Cookbook");
    assert_eq!(book["authors"][0], "The Pigeons of Legejo");
    assert_eq!(book["files"][0]["format"], "epub");
    let href = book["files"][0]["href"].as_str().unwrap().to_string();
    let cover = book["cover"].as_str().unwrap().to_string();
    assert!(href.starts_with(&format!("{address}/")));

    // The cover comes by way of this server. Nothing outside the shelf does.
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let res = app
        .clone()
        .oneshot(Request::builder().uri(format!("/api/linked-shelves/{id}/image?url={}", enc(&cover))).header(header::COOKIE, &b).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("image/"));
    let (status, v) = send(&app, Method::GET, &format!("/api/linked-shelves/{id}/image?url={}", enc(&format!("{there}/api/health"))), Some(&b), None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let outside = json!({ "href": format!("{there}/api/books/{manual}/file"), "title": "x", "format": "epub" });
    let (status, _) = send(&app, Method::POST, &format!("/api/linked-shelves/{id}/import"), Some(&b), Some(outside)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Taken home: a book of Bob's own, and not twice.
    let take = json!({ "href": href, "title": "The Legejo Cookbook", "format": "epub" });
    let (status, v) = send(&app, Method::POST, &format!("/api/linked-shelves/{id}/import"), Some(&b), Some(take.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["title"], "The Legejo Cookbook");
    let mine = v["id"].as_i64().unwrap();
    assert_ne!(mine, manual);
    let (status, v) = send(&app, Method::POST, &format!("/api/linked-shelves/{id}/import"), Some(&b), Some(take.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["book_id"], mine);
    let (status, _) = send(&app, Method::POST, &format!("/api/linked-shelves/{id}/import"), Some(&c), Some(take)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // While it answers, the list says nothing is the matter.
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v[0]["state"], "ok");
    assert!(v[0]["state_since"].is_null());

    // Its instance stops answering: opening it says so, and the list marks it, with since when.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let nowhere = format!("http://{}/api/opds/shared/{}", closed.local_addr().unwrap(), link["token"].as_str().unwrap());
    drop(closed);
    sqlx::query("UPDATE linked_shelves SET url = ? WHERE id = ?").bind(&nowhere).bind(id).execute(&db).await.unwrap();
    let (status, v) = send(&app, Method::GET, &format!("/api/linked-shelves/{id}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(v["error"], "fetch failed");
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v[0]["state"], "unreachable");
    let since = v[0]["state_since"].as_str().unwrap().to_string();
    // Still not answering: the time it began stays.
    send(&app, Method::GET, &format!("/api/linked-shelves/{id}"), Some(&b), None).await;
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v[0]["state_since"], since.as_str());

    // It answers again. The list finds out by itself, in the background, once
    // what it knew has grown old: it answers at once with the old, then with the new.
    sqlx::query("UPDATE linked_shelves SET url = ?, checked_at = '2020-01-01T00:00:00.000Z' WHERE id = ?").bind(&address).bind(id).execute(&db).await.unwrap();
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v[0]["state"], "unreachable", "not waited for");
    let mut state_now = String::new();
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
        state_now = v[0]["state"].as_str().unwrap().to_string();
        if state_now == "ok" {
            assert!(v[0]["state_since"].is_null());
            break;
        }
    }
    assert_eq!(state_now, "ok");

    // The owner takes the link back: the shelf no longer opens, and says why. The list says so too.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/shelves/{shelf}/links/{}", link["id"]), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, v) = send(&app, Method::GET, &format!("/api/linked-shelves/{id}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(v["error"], "missing");
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v[0]["state"], "gone");
    assert!(v[0]["state_since"].is_string());
    // The book Bob took stays his.
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{mine}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::OK);

    // No longer followed.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/linked-shelves/{id}"), Some(&c), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/linked-shelves/{id}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, "/api/linked-shelves", Some(&b), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
}
