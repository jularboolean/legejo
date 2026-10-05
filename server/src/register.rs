//! Self-registration with email verification, gated by the
//! `registration_enabled` setting, plus password reset by mail. Mail is sent
//! over SMTP as configured in mail.rs.

use crate::db::now_ts;
use crate::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn unprocessable(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": msg }))).into_response()
}

pub async fn registration_enabled(state: &AppState) -> Result<bool, sqlx::Error> {
    crate::admin::setting_bool(state, "registration_enabled", false).await
}

pub async fn send_mail(state: &AppState, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
    let config = state
        .mail
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("mail is not configured (LEGEJO_SMTP_* environment)"))?;
    crate::mail::send(config, to, subject, html).await
}

/// Public: lets the login page decide whether to show the register link.
pub async fn enabled(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    let enabled = registration_enabled(&state).await.map_err(|e| internal(e.into()))?;
    Ok(Json(json!({ "enabled": enabled, "mail_configured": state.mail.is_some() })))
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

pub(crate) fn looks_like_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// External base URL derived from X-Forwarded-Proto and Host, as the Kobo
/// endpoints do.
pub(crate) fn base_url(headers: &HeaderMap) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:3000");
    format!("{proto}://{host}")
}

pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<StatusCode, Response> {
    if !registration_enabled(&state).await.map_err(|e| internal(e.into()))? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "registration is disabled" })),
        )
            .into_response());
    }

    let username = req.username.trim();
    let email = req.email.trim().to_string();
    if username.is_empty() || username.len() > 60 {
        return Err(unprocessable("username must be 1-60 characters"));
    }
    if !looks_like_email(&email) {
        return Err(unprocessable("that does not look like an email address"));
    }
    if req.password.len() < 8 {
        return Err(unprocessable("password must be at least 8 characters"));
    }

    let taken: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE LOWER(username) = LOWER($1) OR (email IS NOT NULL AND LOWER(email) = LOWER($2))",
    )
    .bind(username)
    .bind(&email)
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if taken > 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "username or email is already in use" })),
        )
            .into_response());
    }

    let hash = crate::db::hash_password(&req.password).map_err(internal)?;
    let token = crate::auth::new_token();
    let user_id: i64 = match sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, is_admin, email, verify_token, verify_expires)
         VALUES ($1, $2, 0, $3, $4, $5) RETURNING id",
    )
    .bind(username)
    .bind(&hash)
    .bind(&email)
    .bind(&token)
    .bind(crate::db::ts_in_days(2))
    .fetch_one(&state.db)
    .await
    {
        Ok(id) => id,
        // The pre-check races with concurrent registrations; the unique
        // indexes are the actual guard.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "username or email is already in use" })),
            )
                .into_response());
        }
        Err(e) => return Err(internal(e.into())),
    };

    let link = format!("{}/verify?token={token}", base_url(&headers));
    let html = format!(
        "<p>Welcome to Legejo, {username}!</p>\
         <p>Confirm your email address to activate the account:</p>\
         <p><a href=\"{link}\">{link}</a></p>\
         <p>The link is valid for 48 hours. If you did not create this account, ignore this mail.</p>"
    );
    if let Err(e) = send_mail(&state, &email, "Confirm your Legejo account", &html).await {
        // Without the mail the account can never be verified, so remove it.
        tracing::warn!("registration mail failed: {e:#}");
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await;
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "could not send the verification email" })),
        )
            .into_response());
    }

    crate::audit::log(&state, Some((user_id, username)), "user.registered", json!({ "email": email })).await;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    token: String,
}

pub async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, username FROM users WHERE verify_token = $1 AND verify_expires > $2",
    )
    .bind(req.token.trim())
    .bind(now_ts())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let Some((id, username)) = row else {
        return Err((
            StatusCode::GONE,
            Json(json!({ "error": "the link is invalid or has expired" })),
        )
            .into_response());
    };

    sqlx::query("UPDATE users SET verify_token = NULL, verify_expires = NULL WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    crate::audit::log(&state, Some((id, &username)), "user.verified", json!({})).await;
    Ok(Json(json!({ "username": username })))
}

#[derive(Deserialize)]
pub struct ForgotRequest {
    email: String,
}

/// Always answers 204 whether or not the address exists, so the endpoint
/// cannot be used to enumerate accounts. 503 only when mail is not configured.
pub async fn forgot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ForgotRequest>,
) -> Result<StatusCode, Response> {
    if state.mail.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "mail is not configured" })),
        )
            .into_response());
    }
    let email = req.email.trim().to_string();
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, username FROM users WHERE email IS NOT NULL AND LOWER(email) = LOWER($1)",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    if let Some((id, username)) = row {
        let token = crate::auth::new_token();
        sqlx::query("UPDATE users SET reset_token = $1, reset_expires = $2 WHERE id = $3")
            .bind(&token)
            .bind(crate::db::ts_in_hours(1))
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        let link = format!("{}/reset?token={token}", base_url(&headers));
        let html = format!(
            "<p>Hello {username},</p>\
             <p>Someone asked to reset the password for your Legejo account. \
             Follow the link to choose a new one:</p>\
             <p><a href=\"{link}\">{link}</a></p>\
             <p>The link is valid for one hour. If this wasn't you, ignore this mail; \
             your password is unchanged.</p>"
        );
        crate::audit::log(&state, Some((id, &username)), "user.reset_requested", json!({})).await;
        if let Err(e) = send_mail(&state, &email, "Reset your Legejo password", &html).await {
            tracing::warn!("password reset mail failed: {e:#}");
            // Still 204: the response must not reveal whether the address exists.
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct ResetRequest {
    token: String,
    password: String,
}

pub async fn reset(
    State(state): State<AppState>,
    Json(req): Json<ResetRequest>,
) -> Result<StatusCode, Response> {
    if req.password.len() < 8 {
        return Err(unprocessable("password must be at least 8 characters"));
    }
    let row: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM users WHERE reset_token = $1 AND reset_expires > $2",
    )
    .bind(req.token.trim())
    .bind(now_ts())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let Some(id) = row else {
        return Err((
            StatusCode::GONE,
            Json(json!({ "error": "the link is invalid or has expired" })),
        )
            .into_response());
    };

    let hash = crate::db::hash_password(&req.password).map_err(internal)?;
    // Following a link mailed to the address proves ownership, so the reset
    // also completes a pending email verification.
    sqlx::query(
        "UPDATE users SET password_hash = $1, reset_token = NULL, reset_expires = NULL,
                          verify_token = NULL, verify_expires = NULL,
                          invite_token = NULL, invite_expires = NULL
         WHERE id = $2",
    )
    .bind(&hash)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    // Invalidate all existing sessions along with the old password.
    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let name: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    crate::audit::log(&state, Some((id, &name)), "user.password_reset", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}
