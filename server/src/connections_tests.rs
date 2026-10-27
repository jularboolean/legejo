//! Mail, the librarian and the public address set up on the admin page
//! (live.rs): kept, used at once, secrets never handed back, the
//! environment winning, and the test buttons.

use crate::testutil::{add_user, send, test_app};
use axum::http::{Method, StatusCode};
use axum::Router;
use serde_json::json;

async fn admin(db: &sqlx::AnyPool) -> String {
    let (id, cookie) = add_user(db, "alice").await;
    sqlx::query("UPDATE users SET is_admin = 1, email = 'alice@example.org' WHERE id = ?").bind(id).execute(db).await.unwrap();
    cookie
}

/// Serve an app on this machine; returns its address.
async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

#[tokio::test]
async fn mail_is_set_up_in_the_browser_and_used_at_once() {
    let (app, db, state) = test_app().await;
    let cookie = admin(&db).await;
    let (_, c) = send(&app, Method::GET, "/api/admin/connections", Some(&cookie), None).await;
    assert_eq!(c["mail"]["source"], "none");
    let (_, plain) = add_user(&db, "bob").await;
    assert_eq!(send(&app, Method::GET, "/api/admin/connections", Some(&plain), None).await.0, StatusCode::FORBIDDEN);

    // A sender that is no address is refused.
    let form = json!({ "host": "smtp.example.org", "port": 465, "user": "legejo", "password": "s3cret-pass", "tls": "tls", "from": "not an address" });
    assert_eq!(send(&app, Method::PUT, "/api/admin/connections/mail", Some(&cookie), Some(form)).await.0, StatusCode::UNPROCESSABLE_ENTITY);

    let form = json!({ "host": "smtp.example.org", "port": 465, "user": "legejo", "password": "s3cret-pass", "tls": "tls", "from": "Legejo <books@example.org>" });
    let (status, c) = send(&app, Method::PUT, "/api/admin/connections/mail", Some(&cookie), Some(form)).await;
    assert_eq!(status, StatusCode::OK, "{c}");
    assert_eq!((c["mail"]["source"].clone(), c["mail"]["host"].clone(), c["mail"]["port"].clone(), c["mail"]["password_set"].clone()), (json!("admin"), json!("smtp.example.org"), json!(465), json!(true)));
    assert!(!c.to_string().contains("s3cret-pass"), "the password is never handed back");
    // Used at once: registration can now say mail is there.
    assert!(state.live.mail().is_some());
    let (_, r) = send(&app, Method::GET, "/api/register/enabled", None, None).await;
    assert_eq!(r["mail_configured"], true);

    // Saved again without a password: the old one is kept.
    let form = json!({ "host": "smtp.example.org", "port": 587, "user": "legejo", "from": "Legejo <books@example.org>" });
    let (_, c) = send(&app, Method::PUT, "/api/admin/connections/mail", Some(&cookie), Some(form)).await;
    assert_eq!((c["mail"]["port"].clone(), c["mail"]["password_set"].clone()), (json!(587), json!(true)));

    // The test reports what the server said; nothing listens on port 1.
    let form = json!({ "host": "127.0.0.1", "port": 1, "tls": "starttls", "from": "Legejo <books@example.org>" });
    let (status, t) = send(&app, Method::POST, "/api/admin/connections/mail/test", Some(&cookie), Some(form)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((t["ok"].clone(), t["to"].clone()), (json!(false), json!("alice@example.org")));
    assert!(t["error"].as_str().is_some_and(|e| !e.is_empty()));

    // An empty host turns mail off.
    let (_, c) = send(&app, Method::PUT, "/api/admin/connections/mail", Some(&cookie), Some(json!({ "host": "" }))).await;
    assert_eq!((c["mail"]["source"].clone(), c["mail"]["password_set"].clone()), (json!("none"), json!(false)));
    assert!(state.live.mail().is_none());
}

#[tokio::test]
async fn the_environment_wins() {
    let (_, db, mut state) = test_app().await;
    state.live = std::sync::Arc::new(crate::live::Live::fixed(Some(crate::mail::MailConfig::local(25, "Legejo <a@b.test>")), None, Some("https://books.example.org".into())));
    let app = crate::router(state.clone());
    let cookie = admin(&db).await;
    let (_, c) = send(&app, Method::GET, "/api/admin/connections", Some(&cookie), None).await;
    assert_eq!((c["mail"]["source"].clone(), c["public_url"]["source"].clone(), c["librarian"]["source"].clone()), (json!("env"), json!("env"), json!("none")));
    let form = json!({ "host": "smtp.example.org", "from": "Legejo <books@example.org>" });
    assert_eq!(send(&app, Method::PUT, "/api/admin/connections/mail", Some(&cookie), Some(form)).await.0, StatusCode::CONFLICT);
    assert_eq!(send(&app, Method::PUT, "/api/admin/connections/public-url", Some(&cookie), Some(json!({ "url": "https://x.test" }))).await.0, StatusCode::CONFLICT);
    assert_eq!(state.live.public_url().as_deref(), Some("https://books.example.org"));
}

#[tokio::test]
async fn the_librarian_is_set_up_and_tested() {
    let (app, db, state) = test_app().await;
    let cookie = admin(&db).await;
    // A model that answers, and checks the key.
    let model = Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(|headers: axum::http::HeaderMap| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer good-key") {
                (StatusCode::OK, axum::Json(json!({ "model": "test-model", "choices": [{ "message": { "content": "ready" } }] })))
            } else {
                (StatusCode::UNAUTHORIZED, axum::Json(json!({ "error": { "message": "bad key" } })))
            }
        }),
    );
    let base = format!("{}/v1", serve(model).await);

    let (_, t) = send(&app, Method::POST, "/api/admin/connections/librarian/test", Some(&cookie), Some(json!({ "base_url": base, "model": "test-model", "key": "wrong" }))).await;
    assert_eq!(t["ok"], false);
    assert!(t["error"].as_str().unwrap().contains("bad key"), "{t}");
    let (_, t) = send(&app, Method::POST, "/api/admin/connections/librarian/test", Some(&cookie), Some(json!({ "base_url": base, "model": "test-model", "key": "good-key" }))).await;
    assert_eq!(t["ok"], true, "{t}");

    let (status, c) = send(&app, Method::PUT, "/api/admin/connections/librarian", Some(&cookie), Some(json!({ "base_url": base, "model": "test-model", "key": "good-key" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((c["librarian"]["source"].clone(), c["librarian"]["key_set"].clone()), (json!("admin"), json!(true)));
    assert!(!c.to_string().contains("good-key"));
    assert_eq!(state.live.ai().map(|a| a.model).as_deref(), Some("test-model"));
    // Tested again later without the key typed: the saved one is used.
    let (_, t) = send(&app, Method::POST, "/api/admin/connections/librarian/test", Some(&cookie), Some(json!({ "base_url": base, "model": "test-model" }))).await;
    assert_eq!(t["ok"], true);
    // An empty key turns it off.
    send(&app, Method::PUT, "/api/admin/connections/librarian", Some(&cookie), Some(json!({ "key": "" }))).await;
    assert!(state.live.ai().is_none());
}

#[tokio::test]
async fn the_public_address_is_checked_against_this_instance() {
    let (app, db, state) = test_app().await;
    let cookie = admin(&db).await;
    for bad in ["ftp://books.example.org", "https://books.example.org/library", "not a url"] {
        assert_eq!(send(&app, Method::PUT, "/api/admin/connections/public-url", Some(&cookie), Some(json!({ "url": bad }))).await.0, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
    }
    // An address without its scheme gets an answer of its own.
    let (_, e) = send(&app, Method::PUT, "/api/admin/connections/public-url", Some(&cookie), Some(json!({ "url": "localhost:3013" }))).await;
    assert_eq!(e["error"], "scheme");
    let ours = serve(app.clone()).await;
    let (_, t) = send(&app, Method::POST, "/api/admin/connections/public-url/test", Some(&cookie), Some(json!({ "url": ours }))).await;
    assert_eq!((t["ok"].clone(), t["https"].clone()), (json!(true), json!(false)), "{t}");
    // Another Legejo at the address is not this one.
    let (other, _, _) = test_app().await;
    let theirs = serve(other).await;
    let (_, t) = send(&app, Method::POST, "/api/admin/connections/public-url/test", Some(&cookie), Some(json!({ "url": theirs }))).await;
    assert_eq!(t["ok"], false);

    let (_, c) = send(&app, Method::PUT, "/api/admin/connections/public-url", Some(&cookie), Some(json!({ "url": "https://books.example.org/" }))).await;
    assert_eq!((c["public_url"]["source"].clone(), c["public_url"]["value"].clone()), (json!("admin"), json!("https://books.example.org")));
    // Links use it at once, and so does federation.
    assert_eq!(crate::invite::base(&state, &axum::http::HeaderMap::new()), "https://books.example.org");
    assert_eq!(state.fed.config().map(|c| c.host).as_deref(), Some("books.example.org"));
    let (_, f) = send(&app, Method::GET, "/api/admin/federation", Some(&cookie), None).await;
    assert_eq!(f["public_url"], "https://books.example.org");
}
