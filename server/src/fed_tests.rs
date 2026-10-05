//! API tests for federated shelves.

use crate::testutil::{add_book, add_user, send, test_app_fed};
use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

async fn set_mode(db: &sqlx::AnyPool, mode: &str) {
    sqlx::query("INSERT INTO settings (key, value) VALUES ('federation_mode', ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(mode)
        .execute(db)
        .await
        .unwrap();
}

async fn free(db: &sqlx::AnyPool, book: i64) {
    sqlx::query("UPDATE books SET license = 'pd', license_source_url = 'https://runeberg.org/x/', author_death_year = 1912, file_sha256 = 'ab' WHERE id = ?")
        .bind(book)
        .execute(db)
        .await
        .unwrap();
}

#[tokio::test]
async fn everything_is_404_while_off() {
    let (app, db, _) = test_app_fed().await;
    for uri in ["/nodeinfo/2.1", "/.well-known/nodeinfo", "/ap/actor", "/.well-known/webfinger?resource=acct:x@a.test", "/f/x"] {
        let (status, _) = send(&app, Method::GET, uri, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    let (status, _) = send(&app, Method::POST, "/ap/inbox", None, Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    set_mode(&db, "allowlist").await;
    let (status, v) = send(&app, Method::GET, "/nodeinfo/2.1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["software"]["name"], "legejo");
}

#[tokio::test]
async fn a_shelf_federates_only_with_free_books_and_keeps_its_handle() {
    let (app, db, _) = test_app_fed().await;
    let (alice, cookie) = add_user(&db, "alice").await;
    let (free_book, _) = add_book(&db, alice, "Röda rummet").await;
    let (closed, _) = add_book(&db, alice, "Skyddad").await;
    free(&db, free_book).await;
    let (status, v) = send(&app, Method::POST, "/api/shelves", Some(&cookie), Some(json!({ "name": "Klassiker" }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let shelf = v["id"].as_i64().unwrap();
    assert_eq!(v["suggested_slug"], "alice-klassiker");
    for book in [free_book, closed] {
        sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?)").bind(shelf).bind(book).execute(&db).await.unwrap();
    }
    let uri = format!("/api/shelves/{shelf}");
    let fed = json!({ "name": "Klassiker", "visibility": "federated", "ap_slug": "alice-klassiker" });

    // Off: not possible at all.
    let (status, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(fed.clone())).await;
    assert_eq!((status, v["error"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!("federation is off")));

    // At shelf level the book with an unknown license blocks, and nothing changes.
    set_mode(&db, "allowlist").await;
    let (status, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(fed.clone())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(v["blocking"].as_array().unwrap().len(), 1);
    assert_eq!(v["blocking"][0]["id"], closed);
    assert_eq!(v["blocking"][0]["reason"]["code"], "unknown_license");

    sqlx::query("DELETE FROM shelf_books WHERE book_id = ?").bind(closed).execute(&db).await.unwrap();
    let (status, v) = send(&app, Method::PUT, &uri, Some(&cookie), Some(fed)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["handle"], "@alice-klassiker@a.test");

    // The handle never changes once used.
    let (status, v) = send(&app, Method::PUT, &uri, Some(&cookie),
        Some(json!({ "name": "Klassiker", "visibility": "federated", "ap_slug": "annat" }))).await;
    assert_eq!((status, v["error"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!("handle cannot change")));

    // Adding the protected book now fails, and so does making the free one unfree.
    let (status, v) = send(&app, Method::PUT, &format!("/api/books/{closed}"), Some(&cookie),
        Some(json!({ "title": "Skyddad", "shelf_ids": [shelf] }))).await;
    assert_eq!((status, v["error"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!("books not federable")));
    let (status, v) = send(&app, Method::PUT, &format!("/api/books/{free_book}"), Some(&cookie),
        Some(json!({ "title": "Röda rummet", "license": { "license": "copyright" } }))).await;
    assert_eq!((status, v["error"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!("book on federated shelf")));
    let license: Option<String> = sqlx::query_scalar("SELECT license FROM books WHERE id = ?").bind(free_book).fetch_one(&db).await.unwrap();
    assert_eq!(license.as_deref(), Some("pd"), "the refused edit was rolled back");

    // Public endpoints answer for the shelf and its book.
    let (status, v) = send(&app, Method::GET, "/.well-known/webfinger?resource=acct:alice-klassiker@a.test", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["links"][0]["href"], "https://a.test/ap/shelves/alice-klassiker");
    let (status, v): (StatusCode, Value) = send(&app, Method::GET, "/ap/shelves/alice-klassiker/outbox?page=1", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["orderedItems"][0]["object"]["license"], "pd");
}

mod requests {
    //! Undecided instances in allowlist mode, and followed shelves that stop answering.

    use super::set_mode;
    use crate::fed::{requests, Fed, FedConfig};
    use crate::testutil::{add_user, send, test_app};
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{header, Method, Request, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use sqlx::AnyPool;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// A remote instance with one shelf actor; `status` is what it answers.
    #[derive(Clone)]
    struct Remote {
        base: String,
        host: String,
        status: Arc<AtomicU16>,
    }

    impl Remote {
        fn shelf(&self) -> String {
            format!("{}/ap/shelves/classics", self.base)
        }
    }

    async fn remote() -> Remote {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = listener.local_addr().unwrap().to_string();
        let r = Remote { base: format!("http://{host}"), host, status: Arc::new(AtomicU16::new(200)) };
        let app = Router::new()
            .route(
                "/ap/shelves/classics",
                get(|State(r): State<Remote>| async move {
                    let status = StatusCode::from_u16(r.status.load(Ordering::SeqCst)).unwrap();
                    let actor = json!({
                        "id": r.shelf(), "type": "Service", "preferredUsername": "classics", "name": "Classics",
                        "inbox": format!("{}/inbox", r.shelf()), "outbox": format!("{}/outbox", r.shelf()),
                    });
                    (status, Json(actor))
                }),
            )
            .with_state(r.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        r
    }

    /// An instance at http://127.0.0.1 that may talk to other local ports.
    async fn local_instance() -> (Router, AnyPool, crate::AppState) {
        let (_, db, mut state) = test_app().await;
        let config = FedConfig { base: "http://127.0.0.1:1".into(), host: "127.0.0.1:1".into(), allow_private: true };
        state.fed = Fed::new(Some(config)).unwrap();
        set_mode(&db, "allowlist").await;
        (crate::router(state.clone()), db, state)
    }

    async fn admin(db: &AnyPool) -> String {
        let (id, cookie) = add_user(db, "admin").await;
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(id).execute(db).await.unwrap();
        cookie
    }

    async fn follow_state(db: &AnyPool, actor: &str) -> (String, Option<String>) {
        sqlx::query_as("SELECT state, sent_at FROM ap_follows WHERE remote_actor_iri = ?").bind(actor).fetch_one(db).await.unwrap()
    }

    async fn queued(db: &AnyPool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM ap_deliveries").fetch_one(db).await.unwrap()
    }

    #[tokio::test]
    async fn a_follow_of_an_undecided_instance_waits_for_the_admin() {
        let (app, db, _) = local_instance().await;
        let r = remote().await;
        let admin = admin(&db).await;
        let (_, alice) = add_user(&db, "alice").await;

        let (status, item) = send(&app, Method::POST, "/api/fed/follows", Some(&alice), Some(json!({ "handle": r.shelf() }))).await;
        assert_eq!(status, StatusCode::CREATED, "{item}");
        assert_eq!(item["awaiting_approval"], true);
        assert_eq!(queued(&db).await, 0, "nothing is sent before the instance is allowed");
        assert_eq!(follow_state(&db, &r.shelf()).await, ("pending".into(), None));

        // The admin sees who asked, and is told once.
        let (_, pending) = send(&app, Method::GET, "/api/admin/federation/pending", Some(&admin), None).await;
        assert_eq!(pending["count"], 1);
        let (_, overview) = send(&app, Method::GET, "/api/admin/federation/overview", Some(&admin), None).await;
        let request = &overview["requests"][0];
        assert_eq!((request["domain"].as_str(), request["direction"].as_str()), (Some(r.host.as_str()), Some("out")));
        assert_eq!(request["requested_by"], "alice");
        let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'fed.instance_requested'").fetch_one(&db).await.unwrap();
        assert_eq!(logged, 1);
        let (status, _) = send(&app, Method::GET, "/api/admin/federation/pending", Some(&alice), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // Allowing the instance sends the held Follow.
        let uri = format!("/api/admin/federation/instances/{}", r.host);
        let (status, _) = send(&app, Method::PUT, &uri, Some(&admin), Some(json!({ "status": "allowed" }))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(queued(&db).await, 1);
        let (state, sent_at) = follow_state(&db, &r.shelf()).await;
        assert!(state == "pending" && sent_at.is_some());
        let (_, pending) = send(&app, Method::GET, "/api/admin/federation/pending", Some(&admin), None).await;
        assert_eq!(pending["count"], 0);
        let (_, follows) = send(&app, Method::GET, "/api/fed/follows", Some(&alice), None).await;
        assert_eq!(follows[0]["awaiting_approval"], false);
    }

    #[tokio::test]
    async fn dismissing_or_blocking_ends_the_held_follow() {
        let (app, db, _) = local_instance().await;
        let r = remote().await;
        let admin = admin(&db).await;
        let (_, alice) = add_user(&db, "alice").await;
        send(&app, Method::POST, "/api/fed/follows", Some(&alice), Some(json!({ "handle": r.shelf() }))).await;

        let uri = format!("/api/admin/federation/requests/{}", r.host);
        let (status, _) = send(&app, Method::DELETE, &uri, Some(&admin), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(follow_state(&db, &r.shelf()).await.0, "rejected");
        assert_eq!(queued(&db).await, 0);

        // A blocked instance cannot be followed at all.
        let uri = format!("/api/admin/federation/instances/{}", r.host);
        send(&app, Method::PUT, &uri, Some(&admin), Some(json!({ "status": "blocked" }))).await;
        let (status, v) = send(&app, Method::POST, "/api/fed/follows", Some(&alice), Some(json!({ "handle": r.shelf() }))).await;
        assert_eq!((status, v["error"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("domain not allowed")));
    }

    #[tokio::test]
    async fn an_incoming_follow_from_an_undecided_instance_is_recorded() {
        let (app, db, _) = local_instance().await;
        let admin = admin(&db).await;
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let actor = "http://127.0.0.1:9/ap/actor";
        let body = json!({
            "id": "http://127.0.0.1:9/ap/follows/1", "type": "Follow", "actor": actor,
            "object": "http://127.0.0.1:1/ap/shelves/alice-classics",
        })
        .to_string();
        let post = |body: String| {
            let url = reqwest::Url::parse("http://127.0.0.1:1/ap/inbox").unwrap();
            let mut req = Request::builder().method(Method::POST).uri("/ap/inbox").header(header::HOST, "127.0.0.1:1");
            for (name, value) in crate::fed::sig::sign("POST", &url, Some(body.as_bytes()), &format!("{actor}#main-key"), &key) {
                req = req.header(name, value);
            }
            app.clone().oneshot(req.body(Body::from(body)).unwrap())
        };
        for _ in 0..2 {
            assert_eq!(post(body.clone()).await.unwrap().status(), StatusCode::FORBIDDEN);
        }
        let (_, overview) = send(&app, Method::GET, "/api/admin/federation/overview", Some(&admin), None).await;
        let requests = overview["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!((requests[0]["direction"].as_str(), requests[0]["detail"].as_str()), (Some("in"), Some("alice-classics")));
        assert_eq!((requests[0]["requested_by"].as_str(), requests[0]["attempts"].as_i64()), (Some(actor), Some(2)));

        // Other refused activities, and open mode, leave no request.
        let like = body.replace("Follow", "Like").replace("follows/1", "likes/1");
        post(like).await.unwrap();
        assert_eq!(send(&app, Method::GET, "/api/admin/federation/pending", Some(&admin), None).await.1["count"], 1);
    }

    #[tokio::test]
    async fn a_shelf_that_stops_answering_is_unreachable_then_gone() {
        let (app, db, state) = local_instance().await;
        let r = remote().await;
        let admin = admin(&db).await;
        let (_, alice) = add_user(&db, "alice").await;
        let uri = format!("/api/admin/federation/instances/{}", r.host);
        send(&app, Method::PUT, &uri, Some(&admin), Some(json!({ "status": "allowed" }))).await;
        send(&app, Method::POST, "/api/fed/follows", Some(&alice), Some(json!({ "handle": r.shelf() }))).await;
        sqlx::query("UPDATE ap_follows SET state = 'accepted'").execute(&db).await.unwrap();
        let unreachable = |v: &Value| v[0]["unreachable_since"].clone();

        requests::check(&state, &r.shelf()).await;
        let (_, follows) = send(&app, Method::GET, "/api/fed/follows", Some(&alice), None).await;
        assert_eq!((follows[0]["state"].as_str(), unreachable(&follows)), (Some("accepted"), Value::Null));

        // The instance fails: shown as unreachable, but still followed.
        r.status.store(502, Ordering::SeqCst);
        requests::check(&state, &r.shelf()).await;
        let (_, follows) = send(&app, Method::GET, "/api/fed/follows", Some(&alice), None).await;
        assert_eq!(follows[0]["state"], "accepted");
        assert!(unreachable(&follows).is_string());

        // It comes back: the mark is cleared.
        r.status.store(200, Ordering::SeqCst);
        requests::check(&state, &r.shelf()).await;
        let (_, follows) = send(&app, Method::GET, "/api/fed/follows", Some(&alice), None).await;
        assert_eq!(unreachable(&follows), Value::Null);

        // Unreachable for more than a week: treated as gone.
        r.status.store(404, Ordering::SeqCst);
        requests::check(&state, &r.shelf()).await;
        sqlx::query("UPDATE ap_remote_actors SET unreachable_since = '2000-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
        requests::check(&state, &r.shelf()).await;
        assert_eq!(follow_state(&db, &r.shelf()).await.0, "gone");
    }

    #[tokio::test]
    async fn a_deleted_shelf_is_gone_at_once() {
        let (app, db, state) = local_instance().await;
        let r = remote().await;
        let admin = admin(&db).await;
        let (_, alice) = add_user(&db, "alice").await;
        let uri = format!("/api/admin/federation/instances/{}", r.host);
        send(&app, Method::PUT, &uri, Some(&admin), Some(json!({ "status": "allowed" }))).await;
        send(&app, Method::POST, "/api/fed/follows", Some(&alice), Some(json!({ "handle": r.shelf() }))).await;
        r.status.store(410, Ordering::SeqCst);
        requests::check(&state, &r.shelf()).await;
        assert_eq!(follow_state(&db, &r.shelf()).await.0, "gone");
    }
}
