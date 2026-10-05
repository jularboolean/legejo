//! Login throttling against password guessing.
//!
//! Middleware in front of the web login and OPDS Basic auth; the password
//! checks themselves are unchanged. Each failed attempt (401) is recorded per
//! key. Once a key reaches its limit within the window, further attempts,
//! correct or not, get 429 until the window has passed. Keys:
//!
//! - without a client-IP header: the username;
//! - with one (LEGEJO_CLIENT_IP_HEADER): username + IP, plus the IP alone
//!   and the username alone, both with the higher `max_failures_ip`.
//!
//! Counts are stored in the database so they are shared between server
//! processes, e.g. during a rolling deploy.

use crate::db::now_ts;
use crate::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde_json::json;

fn client_ip(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let name = state.settings.limits.ip_header.as_deref()?;
    let value = headers.get(name)?.to_str().ok()?;
    // X-Forwarded-For: the first entry is the client as the outermost proxy saw it.
    let ip = value.split(',').next()?.trim();
    (!ip.is_empty() && ip.len() <= 64).then(|| ip.to_string())
}

/// (key, limit) pairs for one attempt.
fn keys(state: &AppState, username: &str, ip: Option<&str>) -> Vec<(String, i64)> {
    let l = &state.settings.limits;
    let user = username.trim().to_lowercase();
    match ip {
        None => vec![(format!("user:{user}"), l.max_failures)],
        Some(ip) => vec![
            (format!("user:{user}|ip:{ip}"), l.max_failures),
            (format!("user:{user}"), l.max_failures_ip),
            (format!("ip:{ip}"), l.max_failures_ip),
        ],
    }
}

fn cutoff(state: &AppState) -> String {
    (time::OffsetDateTime::now_utc() - time::Duration::minutes(state.settings.limits.window_minutes))
        .format(&crate::db::TS_FORMAT)
        .unwrap_or_default()
}

async fn count(state: &AppState, key: &str, since: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM auth_failures WHERE key = $1 AND at > $2")
        .bind(key)
        .bind(since)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
}

async fn blocked(state: &AppState, keys: &[(String, i64)]) -> bool {
    let since = cutoff(state);
    for (key, limit) in keys {
        if *limit > 0 && count(state, key, &since).await >= *limit {
            return true;
        }
    }
    false
}

async fn record_failure(state: &AppState, username: &str, keys: &[(String, i64)]) {
    let now = now_ts();
    let since = cutoff(state);
    for (key, limit) in keys {
        if *limit == 0 {
            continue;
        }
        if let Err(e) = sqlx::query("INSERT INTO auth_failures (key, at) VALUES ($1, $2)")
            .bind(key)
            .bind(&now)
            .execute(&state.db)
            .await
        {
            tracing::warn!("ratelimit: could not record a failure: {e}");
            return;
        }
        // Audit once, when the per-username key reaches its limit.
        if count(state, key, &since).await == *limit && key.starts_with("user:") && !key.contains("|ip:") {
            crate::audit::log(state, None, "user.login_blocked", json!({ "username": username.trim() })).await;
        }
    }
}

fn too_many(state: &AppState, json_body: bool) -> Response {
    let retry = (state.settings.limits.window_minutes * 60).to_string();
    if json_body {
        (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, retry)], Json(json!({ "error": "too many attempts" }))).into_response()
    } else {
        (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, retry)], "too many failed logins, try again later").into_response()
    }
}

fn enabled(state: &AppState) -> bool {
    let l = &state.settings.limits;
    l.max_failures > 0 || (l.ip_header.is_some() && l.max_failures_ip > 0)
}

/// Around POST /api/auth/login (JSON body with `username`).
pub async fn login(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !enabled(&state) {
        return next.run(req).await;
    }
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 64 * 1024).await {
        Ok(b) => b,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let username = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|v| v.get("username").and_then(|u| u.as_str()).map(str::to_string))
        .unwrap_or_default();
    let ip = client_ip(&state, &parts.headers);
    let keys = keys(&state, &username, ip.as_deref());
    if blocked(&state, &keys).await {
        return too_many(&state, true);
    }
    let res = next.run(Request::from_parts(parts, Body::from(bytes))).await;
    if res.status() == StatusCode::UNAUTHORIZED {
        record_failure(&state, &username, &keys).await;
    }
    res
}

/// Around the OPDS catalog (HTTP Basic auth). Requests without credentials,
/// typically a reader app's first request, do not count as failures.
pub async fn basic(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if !enabled(&state) {
        return next.run(req).await;
    }
    let username = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| base64::engine::general_purpose::STANDARD.decode(v).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.split_once(':').map(|(u, _)| u.to_string()));
    let Some(username) = username else {
        return next.run(req).await;
    };
    let ip = client_ip(&state, req.headers());
    let keys = keys(&state, &username, ip.as_deref());
    if blocked(&state, &keys).await {
        return too_many(&state, false);
    }
    let res = next.run(req).await;
    if res.status() == StatusCode::UNAUTHORIZED {
        record_failure(&state, &username, &keys).await;
    }
    res
}

/// Delete failures older than a day. Called by the daily prune task.
pub async fn prune(state: &AppState) {
    let cutoff = crate::db::ts_in_days(-1);
    if let Err(e) = sqlx::query("DELETE FROM auth_failures WHERE at < $1").bind(cutoff).execute(&state.db).await {
        tracing::warn!("ratelimit: prune failed: {e}");
    }
}

/// Failures within the current window (for /metrics).
pub async fn recent_failures(state: &AppState) -> i64 {
    // Every failure has exactly one plain "user:<name>" row.
    sqlx::query_scalar("SELECT COUNT(*) FROM auth_failures WHERE at > $1 AND key LIKE 'user:%' AND key NOT LIKE '%|ip:%'")
        .bind(cutoff(state))
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
}
