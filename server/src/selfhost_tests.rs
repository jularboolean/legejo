//! Operator features: login throttling, /metrics and the import folder.

use crate::settings::{ImportDir, Metrics, Settings};
use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::Engine;
use serde_json::json;
use sqlx::AnyPool;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn app_with(settings: Settings) -> (Router, AnyPool, crate::AppState) {
    let (_, db, mut state) = test_app().await;
    state.settings = Arc::new(settings);
    (crate::router(state.clone()), db, state)
}

async fn user_with_password(db: &AnyPool, name: &str, password: &str) {
    let (id, _) = add_user(db, name).await;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(crate::db::hash_password(password).unwrap())
        .bind(id)
        .execute(db)
        .await
        .unwrap();
}

async fn login(app: &Router, user: &str, password: &str, ip: Option<&str>) -> StatusCode {
    let mut req = Request::builder().method(Method::POST).uri("/api/auth/login").header(header::CONTENT_TYPE, "application/json");
    if let Some(ip) = ip {
        req = req.header("x-real-ip", ip);
    }
    let body = json!({ "username": user, "password": password }).to_string();
    app.clone().oneshot(req.body(Body::from(body)).unwrap()).await.unwrap().status()
}

async fn opds(app: &Router, user: &str, password: &str) -> StatusCode {
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    let req = Request::builder().uri("/api/opds").header(header::AUTHORIZATION, format!("Basic {auth}")).body(Body::empty()).unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

fn limits(max: i64, ip_header: Option<&str>) -> Settings {
    let mut s = Settings::default();
    s.limits.max_failures = max;
    s.limits.max_failures_ip = max * 2;
    s.limits.ip_header = ip_header.map(Into::into);
    s
}

#[tokio::test]
async fn password_guessing_is_throttled_per_username() {
    let (app, db, _) = app_with(limits(3, None)).await;
    user_with_password(&db, "carol", "rätt lösenord").await;
    user_with_password(&db, "dave", "daves lösenord").await;

    // Below the limit the correct password still works.
    assert_eq!(login(&app, "carol", "fel", None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&app, "carol", "fel", None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&app, "carol", "rätt lösenord", None).await, StatusCode::OK);
    // The third failure reaches the limit; now even the correct one is refused.
    assert_eq!(login(&app, "CAROL", "fel", None).await, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&app, "carol", "rätt lösenord", None).await, StatusCode::TOO_MANY_REQUESTS);
    // OPDS shares the count, and other accounts are unaffected.
    assert_eq!(opds(&app, "carol", "rätt lösenord").await, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(login(&app, "dave", "daves lösenord", None).await, StatusCode::OK);
    assert_eq!(opds(&app, "dave", "daves lösenord").await, StatusCode::OK);

    // Audited once.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'user.login_blocked'").fetch_one(&db).await.unwrap();
    assert_eq!(n, 1);

    // Once the window has passed, login works again.
    sqlx::query("UPDATE auth_failures SET at = '2000-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
    assert_eq!(login(&app, "carol", "rätt lösenord", None).await, StatusCode::OK);
}

#[tokio::test]
async fn opds_without_credentials_is_not_a_failure_and_zero_turns_it_off() {
    let (app, db, _) = app_with(limits(1, None)).await;
    user_with_password(&db, "carol", "rätt").await;
    for _ in 0..3 {
        let req = Request::builder().uri("/api/opds").body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(opds(&app, "carol", "rätt").await, StatusCode::OK);

    let (app, db, _) = app_with(limits(0, None)).await;
    user_with_password(&db, "carol", "rätt").await;
    for _ in 0..20 {
        assert_eq!(login(&app, "carol", "fel", None).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(login(&app, "carol", "rätt", None).await, StatusCode::OK);
}

#[tokio::test]
async fn with_a_client_ip_an_attacker_does_not_lock_out_the_owner() {
    let (app, db, _) = app_with(limits(3, Some("x-real-ip"))).await;
    user_with_password(&db, "carol", "rätt").await;
    for _ in 0..3 {
        assert_eq!(login(&app, "carol", "fel", Some("203.0.113.9")).await, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(login(&app, "carol", "rätt", Some("203.0.113.9")).await, StatusCode::TOO_MANY_REQUESTS);
    // The owner from another address is unaffected.
    assert_eq!(login(&app, "carol", "rätt", Some("198.51.100.4")).await, StatusCode::OK);
    // The attacking address is also stopped across usernames (limit 6).
    for name in ["a", "b", "c"] {
        login(&app, name, "x", Some("203.0.113.9")).await;
    }
    assert_eq!(login(&app, "nobody", "x", Some("203.0.113.9")).await, StatusCode::TOO_MANY_REQUESTS);
}

async fn get_metrics(app: &Router, token: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder().uri("/metrics");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn metrics_are_off_open_or_behind_a_token() {
    let (app, _, _) = test_app().await;
    assert_eq!(get_metrics(&app, None).await.0, StatusCode::NOT_FOUND);

    let mut s = Settings::default();
    s.metrics = Metrics::Token("t0ken".into());
    let (app, db, _) = app_with(s).await;
    let (owner, _) = add_user(&db, "carol").await;
    crate::testutil::add_book(&db, owner, "Röda rummet").await;
    assert_eq!(get_metrics(&app, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(get_metrics(&app, Some("fel")).await.0, StatusCode::UNAUTHORIZED);
    let (status, body) = get_metrics(&app, Some("t0ken")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\nlegejo_users 1\n"), "{body}");
    assert!(body.contains("\nlegejo_books 1\n"), "{body}");
    assert!(body.contains("legejo_exports{status=\"queued\"} 0"), "{body}");
    assert!(body.contains("legejo_info{version="), "{body}");

    let mut s = Settings::default();
    s.metrics = Metrics::Open;
    let (app, _, _) = app_with(s).await;
    assert_eq!(get_metrics(&app, None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn upload_limit_is_configurable() {
    let mut s = Settings::default();
    s.max_upload_bytes = 1024;
    let (app, _, _) = app_with(s).await;
    let (status, _) = send(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "x".repeat(2000), "password": "y" }))).await;
    // Bodies over the limit are refused before any handler runs.
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_import_folder_adds_books_once_and_sorts_the_rest() {
    let dir = std::env::temp_dir().join(format!("legejo-import-{}", crate::books::new_uuid()));
    std::fs::create_dir_all(dir.join("Strindberg")).unwrap();
    std::fs::write(dir.join("Strindberg/röda rummet.epub"), crate::library_tests::epub("Röda rummet", "urn:rr")).unwrap();
    std::fs::write(dir.join("hemsöborna.EPUB"), crate::library_tests::epub("Hemsöborna", "urn:hb")).unwrap();
    std::fs::write(dir.join("trasig.epub"), b"not a zip").unwrap();
    std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();

    let config = ImportDir { dir: dir.clone(), user: Some("carol".into()), interval_secs: 60, delete: false };
    let mut s = Settings::default();
    s.import = Some(config.clone());
    let (_, db, state) = app_with(s).await;
    add_user(&db, "admin").await;
    let (carol, _) = add_user(&db, "carol").await;

    // Fresh files wait until they have settled.
    assert_eq!(crate::importdir::scan(&state, &config, Duration::from_secs(3600)).await.unwrap(), 0);
    assert_eq!(crate::importdir::pending(&dir), 3);

    assert_eq!(crate::importdir::scan(&state, &config, Duration::ZERO).await.unwrap(), 2);
    let titles: Vec<String> = sqlx::query_scalar("SELECT title FROM books WHERE owner_id = $1 ORDER BY title")
        .bind(carol)
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(titles, ["Hemsöborna", "Röda rummet"]);
    assert!(dir.join("imported/röda rummet.epub").exists());
    assert!(dir.join("imported/hemsöborna.EPUB").exists());
    assert!(dir.join("failed/trasig.epub").exists());
    assert!(dir.join("notes.txt").exists());
    assert_eq!(crate::importdir::pending(&dir), 0);

    // The same file again is a duplicate, not a second book.
    std::fs::write(dir.join("igen.epub"), crate::library_tests::epub("Röda rummet", "urn:rr")).unwrap();
    assert_eq!(crate::importdir::scan(&state, &config, Duration::ZERO).await.unwrap(), 0);
    assert!(dir.join("duplicates/igen.epub").exists());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books").fetch_one(&db).await.unwrap();
    assert_eq!(n, 2);
    let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'book.folder_imported'").fetch_one(&db).await.unwrap();
    assert_eq!(logged, 2);

    // Without LEGEJO_IMPORT_USER the oldest admin gets the books; delete mode removes the file.
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'admin'").execute(&db).await.unwrap();
    let config = ImportDir { user: None, delete: true, ..config };
    std::fs::write(dir.join("ny.epub"), crate::library_tests::epub("Fröken Julie", "urn:fj")).unwrap();
    assert_eq!(crate::importdir::scan(&state, &config, Duration::ZERO).await.unwrap(), 1);
    let owner: String = sqlx::query_scalar("SELECT u.username FROM books b JOIN users u ON u.id = b.owner_id WHERE b.title = 'Fröken Julie'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(owner, "admin");
    assert!(!dir.join("ny.epub").exists() && !dir.join("imported/ny.epub").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

async fn login_cookie(app: &Router, user: &str, password: &str) -> String {
    let body = json!({ "username": user, "password": password }).to_string();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    res.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string()
}

#[tokio::test]
async fn session_cookies_are_secure_when_configured() {
    let mut s = Settings::default();
    s.secure_cookies = true;
    let (app, db, _) = app_with(s).await;
    user_with_password(&db, "carol", "secret password").await;
    let cookie = login_cookie(&app, "carol", "secret password").await;
    assert!(cookie.contains("Secure"), "{cookie}");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"), "{cookie}");

    let (app, db, _) = app_with(Settings::default()).await;
    user_with_password(&db, "carol", "secret password").await;
    assert!(!login_cookie(&app, "carol", "secret password").await.contains("Secure"));
}
