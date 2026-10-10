//! Shared helpers for API tests: an app on a fresh in-memory database.

use crate::db::Backend;
use crate::{router, AppState};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::AnyPool;
use tower::ServiceExt;

/// App + pool + state; the data directory is a fresh temp dir per test.
pub async fn test_app() -> (Router, AnyPool, AppState) {
    test_app_with(crate::fed::Fed::disabled()).await
}

/// As test_app, but with LEGEJO_PUBLIC_URL set to https://a.test.
pub async fn test_app_fed() -> (Router, AnyPool, AppState) {
    let config = crate::fed::FedConfig { base: "https://a.test".into(), host: "a.test".into(), allow_private: false };
    test_app_with(crate::fed::Fed::new(Some(config)).unwrap()).await
}

/// As test_app, but allowed to fetch from this machine over plain http, the
/// way LEGEJO_FED_ALLOW_PRIVATE=1 sets it up: for tests that run a server of
/// their own to fetch from.
pub async fn test_app_private() -> (Router, AnyPool, AppState) {
    let config = crate::fed::FedConfig { base: "http://127.0.0.1".into(), host: "127.0.0.1".into(), allow_private: true };
    test_app_with(crate::fed::Fed::new(Some(config)).unwrap()).await
}

async fn test_app_with(fed: std::sync::Arc<crate::fed::Fed>) -> (Router, AnyPool, AppState) {
    build(fed, crate::settings::Settings::default()).await
}

async fn build(fed: std::sync::Arc<crate::fed::Fed>, settings: crate::settings::Settings) -> (Router, AnyPool, AppState) {
    sqlx::any::install_default_drivers();
    // One connection that never retires: every query must see the same
    // in-memory database for the whole test.
    let db = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await.unwrap();
    crate::db::SQLITE_MIGRATOR.run(&db).await.unwrap();
    let data_dir = std::env::temp_dir().join(format!("legejo-test-{}", crate::books::new_uuid()));
    for sub in ["books", "covers"] {
        std::fs::create_dir_all(data_dir.join(sub)).unwrap();
    }
    let state = AppState {
        db: db.clone(),
        backend: Backend::Sqlite,
        data_dir,
        kepubify: None,
        http: reqwest::Client::new(),
        mail: None,
        thumb_gate: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
        fed,
        export_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
        oidc: None,
        settings: std::sync::Arc::new(settings),
    };
    (router(state.clone()), db, state)
}

/// Creates a user with a live session; returns (user id, Cookie header value).
pub async fn add_user(db: &AnyPool, name: &str) -> (i64, String) {
    let id: i64 = sqlx::query_scalar("INSERT INTO users (username, password_hash) VALUES (?, 'x') RETURNING id")
        .bind(name)
        .fetch_one(db)
        .await
        .unwrap();
    let token = format!("session-{name}");
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, '2999-01-01T00:00:00.000Z')")
        .bind(&token)
        .bind(id)
        .execute(db)
        .await
        .unwrap();
    (id, format!("{}={token}", crate::auth::SESSION_COOKIE))
}

/// A book row (no file on disk unless the test writes one); returns (id, uuid).
pub async fn add_book(db: &AnyPool, owner_id: i64, title: &str) -> (i64, String) {
    let uuid = format!("uuid-{owner_id}-{}", title.replace(' ', "-"));
    let id = sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, ?, 1) RETURNING id")
        .bind(&uuid)
        .bind(owner_id)
        .bind(title)
        .fetch_one(db)
        .await
        .unwrap();
    (id, uuid)
}

pub async fn send(app: &Router, method: Method, uri: &str, cookie: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        req = req.header(header::COOKIE, cookie);
    }
    let req = match body {
        Some(body) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
