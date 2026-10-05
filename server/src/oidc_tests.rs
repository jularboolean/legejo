//! OIDC login end to end against a fake provider on a local port.

use crate::oidc::test_support::TestKey;
use crate::oidc::{Oidc, OidcConfig};
use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Method, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

/// What the fake provider will put in the next ID token.
#[derive(Default)]
struct Next {
    nonce: String,
    challenge: String,
    claims: Value,
}

#[derive(Clone)]
struct Provider {
    base: String,
    key: Arc<TestKey>,
    next: Arc<Mutex<Next>>,
}

async fn provider() -> Provider {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let p = Provider { base, key: Arc::new(TestKey::new()), next: Arc::default() };
    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(|State(p): State<Provider>| async move {
                Json(json!({
                    "issuer": p.base,
                    "authorization_endpoint": format!("{}/authorize", p.base),
                    "token_endpoint": format!("{}/token", p.base),
                    "jwks_uri": format!("{}/jwks", p.base),
                }))
            }),
        )
        .route("/jwks", get(|State(p): State<Provider>| async move { Json(p.key.jwks_json("k1")) }))
        .route(
            "/token",
            post(|State(p): State<Provider>, headers: axum::http::HeaderMap, Form(f): Form<HashMap<String, String>>| async move {
                let next = p.next.lock().unwrap();
                // Client authentication and PKCE, as a real provider checks them.
                let auth = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
                let expected = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("legejo:hemligt"));
                let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                if auth != expected
                    || f.get("code").map(String::as_str) != Some("kod")
                    || URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != next.challenge
                {
                    return (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant" })));
                }
                let mut claims = json!({
                    "iss": p.base, "aud": "legejo", "exp": 4_000_000_000i64, "iat": 1, "nonce": next.nonce,
                });
                for (k, v) in next.claims.as_object().unwrap() {
                    claims[k] = v.clone();
                }
                (StatusCode::OK, Json(json!({ "id_token": p.key.sign("k1", &claims), "access_token": "a", "token_type": "Bearer" })))
            }),
        )
        .with_state(p.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    p
}

fn config(p: &Provider, auto_create: bool) -> OidcConfig {
    OidcConfig {
        issuer: p.base.clone(),
        client_id: "legejo".into(),
        client_secret: "hemligt".into(),
        name: "Pocket ID".into(),
        auto_create,
        trust_email: false,
        admin_group: Some("legejo-admins".into()),
        scopes: "openid profile email groups".into(),
    }
}

async fn raw(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, String, Vec<String>) {
    let mut req = Request::builder().method(Method::GET).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let location = res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let cookies = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap().to_string())
        .collect();
    (res.status(), location, cookies)
}

/// Runs start → (provider) → callback with the given claims; returns the
/// callback's redirect and the session cookie, if one was set.
async fn login(app: &Router, p: &Provider, claims: Value, session: Option<&str>, link: bool) -> (String, Option<String>) {
    let (status, location, cookies) = raw(app, if link { "/api/auth/oidc/start?link=1" } else { "/api/auth/oidc/start" }, session).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{location}");
    let url = url::Url::parse(&location).unwrap();
    assert!(location.starts_with(&format!("{}/authorize", p.base)));
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert!(q["redirect_uri"].ends_with("/api/auth/oidc/callback"));
    *p.next.lock().unwrap() = Next { nonce: q["nonce"].clone(), challenge: q["code_challenge"].clone(), claims };
    let flow = cookies.iter().find(|c| c.starts_with("legejo_oidc=")).unwrap().clone();
    let cookie = match session {
        Some(s) => format!("{flow}; {s}"),
        None => flow,
    };
    let (status, location, cookies) = raw(app, &format!("/api/auth/oidc/callback?code=kod&state={}", q["state"]), Some(&cookie)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let session = cookies.into_iter().find(|c| c.starts_with("legejo_session=") && c.len() > "legejo_session=".len());
    (location, session)
}

async fn me(app: &Router, session: &str) -> Value {
    let (status, body) = send(app, Method::GET, "/api/auth/me", Some(session), None).await;
    assert_eq!(status, StatusCode::OK);
    body
}

#[tokio::test]
async fn off_unless_configured() {
    let (app, _, _) = test_app().await;
    let (_, body) = send(&app, Method::GET, "/api/auth/config", None, None).await;
    assert_eq!(body, json!({ "oidc": null }));
    assert_eq!(raw(&app, "/api/auth/oidc/start", None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(raw(&app, "/api/auth/oidc/callback?code=x&state=y", None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn logs_in_existing_accounts_by_verified_email_and_remembers_them() {
    let (_, db, mut state) = test_app().await;
    let p = provider().await;
    state.oidc = Oidc::new(Some(config(&p, false)));
    let app = crate::router(state);
    let (carol, _) = add_user(&db, "carol").await;
    sqlx::query("UPDATE users SET email = 'carol@example.org' WHERE id = $1").bind(carol).execute(&db).await.unwrap();

    let (_, body) = send(&app, Method::GET, "/api/auth/config", None, None).await;
    assert_eq!(body["oidc"]["name"], "Pocket ID");

    // An unverified email links nothing.
    let unverified = json!({ "sub": "u-1", "email": "carol@example.org", "email_verified": false });
    let (location, session) = login(&app, &p, unverified, None, false).await;
    assert_eq!(location, "/login?oidc=no_account");
    assert!(session.is_none());

    let verified = json!({ "sub": "u-1", "email": "CAROL@example.org", "email_verified": true });
    let (location, session) = login(&app, &p, verified, None, false).await;
    assert_eq!(location, "/");
    let session = session.expect("a session");
    assert_eq!(me(&app, &session).await["username"], "carol");
    assert_eq!(me(&app, &session).await["is_admin"], false);

    // Next time the subject alone is enough, even if the email changed.
    let renamed = json!({ "sub": "u-1", "email": "other@example.org", "groups": ["legejo-admins"] });
    let (location, session) = login(&app, &p, renamed, None, false).await;
    assert_eq!(location, "/");
    let me_now = me(&app, &session.unwrap()).await;
    assert_eq!(me_now["username"], "carol");
    assert_eq!(me_now["is_admin"], true, "the admin group grants admin");
}

#[tokio::test]
async fn rejects_a_forged_state_and_creates_accounts_only_when_allowed() {
    let (_, db, mut state) = test_app().await;
    let p = provider().await;
    state.oidc = Oidc::new(Some(config(&p, true)));
    let app = crate::router(state);
    add_user(&db, "dave").await;

    // A callback whose state does not match the browser's flow cookie.
    let (_, _, cookies) = raw(&app, "/api/auth/oidc/start", None).await;
    let flow = cookies.iter().find(|c| c.starts_with("legejo_oidc=")).unwrap();
    let (status, location, cookies) = raw(&app, "/api/auth/oidc/callback?code=kod&state=forged", Some(flow)).await;
    assert_eq!((status, location.as_str()), (StatusCode::SEE_OTHER, "/login?oidc=failed"));
    assert!(!cookies.iter().any(|c| c.starts_with("legejo_session=") && c.len() > 16));
    // And one with no flow cookie at all.
    assert_eq!(raw(&app, "/api/auth/oidc/callback?code=kod&state=x", None).await.1, "/login?oidc=failed");

    // Auto-create: the preferred username is taken, so a suffix is added.
    let new = json!({ "sub": "u-9", "preferred_username": "dave", "email": "dave2@example.org", "email_verified": true });
    let (location, session) = login(&app, &p, new, None, false).await;
    assert_eq!(location, "/");
    let me_now = me(&app, &session.unwrap()).await;
    assert_eq!(me_now["username"], "dave-2");
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM users WHERE username = 'dave-2'").fetch_one(&db).await.unwrap();
    assert_eq!(email.as_deref(), Some("dave2@example.org"));
}

#[tokio::test]
async fn a_logged_in_user_can_link_their_account() {
    let (_, db, mut state) = test_app().await;
    let p = provider().await;
    state.oidc = Oidc::new(Some(config(&p, false)));
    let app = crate::router(state);
    let (_, bob) = add_user(&db, "bob").await;
    let (_, carl) = add_user(&db, "carl").await;

    let (_, body) = send(&app, Method::GET, "/api/account/oidc", Some(&bob), None).await;
    assert_eq!(body, json!({ "name": "Pocket ID", "linked": false }));

    let (location, _) = login(&app, &p, json!({ "sub": "b-1" }), Some(&bob), true).await;
    assert_eq!(location, "/account?oidc=linked");
    let (_, body) = send(&app, Method::GET, "/api/account/oidc", Some(&bob), None).await;
    assert_eq!(body["linked"], true);

    // Now the identity logs in as bob, without email.
    let (location, session) = login(&app, &p, json!({ "sub": "b-1" }), None, false).await;
    assert_eq!(location, "/");
    assert_eq!(me(&app, &session.unwrap()).await["username"], "bob");

    // Someone else cannot take it over.
    let (location, _) = login(&app, &p, json!({ "sub": "b-1" }), Some(&carl), true).await;
    assert_eq!(location, "/account?oidc=already_linked");

    // A link flow without a session does nothing.
    let (location, session) = login(&app, &p, json!({ "sub": "b-2" }), Some("legejo_session=nope"), true).await;
    assert_eq!((location.as_str(), session), ("/account?oidc=failed", None));
}
