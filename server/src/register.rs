//! Self-registration with email verification, gated by the
//! `registration_enabled` setting, plus password reset by mail. Mail is sent
//! over SMTP as configured in mail.rs.
//!
//! Without mail, a reset link can be handed over instead: an admin makes one
//! for a user on the Admin page, and `legejo reset-password <user>` makes one
//! from the shell, for when no admin can log in. Both lead to the same reset
//! page as the mailed link, and are valid for HANDED_RESET_HOURS.

use crate::auth::AdminUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
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

/// The base URL guessed from X-Forwarded-Proto and Host, for an instance
/// without LEGEJO_PUBLIC_URL. Only a fallback: a link built from request
/// headers is whatever the sender put there, so anything mailed goes through
/// invite::base, which takes LEGEJO_PUBLIC_URL first.
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

    let link = format!("{}/verify?token={token}", crate::invite::base(&state, &headers));
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
        let token = issue_reset_token(&state.db, id, 1).await.map_err(|e| internal(e.into()))?;
        let link = format!("{}/reset?token={token}", crate::invite::base(&state, &headers));
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

/// A link given by hand may wait a while before it is used.
pub(crate) const HANDED_RESET_HOURS: i64 = 24;

/// A new reset token for the user, replacing any earlier one.
pub(crate) async fn issue_reset_token(db: &sqlx::AnyPool, user_id: i64, hours: i64) -> Result<String, sqlx::Error> {
    let token = crate::auth::new_token();
    sqlx::query("UPDATE users SET reset_token = $1, reset_expires = $2 WHERE id = $3")
        .bind(&token)
        .bind(crate::db::ts_in_hours(hours))
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(token)
}

/// POST /api/admin/users/{id}/reset: a reset link for the admin to pass on.
/// It is never mailed: this is the way for an instance without mail, and
/// with mail the user can ask for one at the login page. An invited user
/// who has not chosen a password gets a renewed invitation instead.
pub async fn admin_reset(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Response> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT username, CAST(CASE WHEN invite_token IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let Some((username, invited)) = row else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response());
    };
    if invited != 0 {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "the invitation is still open" }))).into_response());
    }
    let token = issue_reset_token(&state.db, id, HANDED_RESET_HOURS).await.map_err(|e| internal(e.into()))?;
    let link = format!("{}/reset?token={token}", crate::invite::base(&state, &headers));
    tracing::info!("reset: {} made a reset link for {username}", admin.0.username);
    crate::audit::log(&state, crate::audit::by(&admin.0), "user.reset_link", json!({ "user_id": id, "username": username })).await;
    Ok(Json(json!({ "username": username, "link": link, "hours": HANDED_RESET_HOURS })))
}

/// `legejo reset-password <user>`: prints a reset link for the user, made
/// straight in the database, for when nobody can log in as admin. The link
/// starts with LEGEJO_PUBLIC_URL when it is set; otherwise only its path is
/// printed, to be put after the instance's address.
/// Returns the user's name as stored and the link.
pub async fn reset_password_command(db: &sqlx::AnyPool, username: &str) -> anyhow::Result<(String, String)> {
    let row: Option<(i64, String)> = sqlx::query_as("SELECT id, username FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(username.trim())
        .fetch_optional(db)
        .await?;
    let Some((id, name)) = row else {
        anyhow::bail!("no user named {username}");
    };
    let token = issue_reset_token(db, id, HANDED_RESET_HOURS).await?;
    crate::audit::log_db(db, None, "user.reset_link", json!({ "user_id": id, "username": name, "from": "command line" })).await;
    let base = crate::settings::var_lossy("LEGEJO_PUBLIC_URL").map(|u| u.trim().trim_end_matches('/').to_string()).unwrap_or_default();
    Ok((name, format!("{base}/reset?token={token}")))
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    #[tokio::test]
    async fn mailed_links_come_from_the_public_url_not_the_host_header() {
        use axum::body::Body;
        use axum::http::{header, Request};
        use tower::ServiceExt;
        let (_, db, mut state) = crate::testutil::test_app_fed().await;
        let (port, sink) = crate::kindle_tests::mail_sink().await;
        state.mail = Some(crate::mail::MailConfig::local(port, "Legejo <mail@legejo.test>"));
        let app = crate::router(state.clone());
        let (ada, _) = add_user(&db, "ada").await;
        sqlx::query("UPDATE users SET email = 'ada@example.org' WHERE id = ?").bind(ada).execute(&db).await.unwrap();

        // A request with a forged Host must not decide where the link points.
        let req = Request::builder()
            .method("POST")
            .uri("/api/register/forgot")
            .header(header::HOST, "evil.example")
            .header("x-forwarded-proto", "http")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({ "email": "ada@example.org" }).to_string()))
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let (to, message) = sink.await.unwrap();
        assert_eq!(to, "ada@example.org");
        // Quoted-printable undone, in case the mailer wrapped the lines.
        let text = message.replace("=\n", "").replace("=3D", "=");
        assert!(text.contains("https://a.test/reset?token="), "{text}");
        assert!(!text.contains("evil.example"), "{text}");
    }

    #[tokio::test]
    async fn reset_link_without_mail() {
        let (app, db, _) = test_app().await;
        let (admin, admin_cookie) = add_user(&db, "chef").await;
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin).execute(&db).await.unwrap();
        let (ada, ada_cookie) = add_user(&db, "ada").await;

        // Only an admin makes one, and not for an invitation still open.
        let (status, _) = send(&app, Method::POST, &format!("/api/admin/users/{admin}/reset"), Some(&ada_cookie), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _) = send(&app, Method::POST, "/api/admin/users/9999/reset", Some(&admin_cookie), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, invite) = send(&app, Method::POST, "/api/admin/invites", Some(&admin_cookie), Some(json!({ "username": "ny" }))).await;
        let invited = invite["id"].as_i64().unwrap();
        let (status, _) = send(&app, Method::POST, &format!("/api/admin/users/{invited}/reset"), Some(&admin_cookie), None).await;
        assert_eq!(status, StatusCode::CONFLICT);

        let (status, v) = send(&app, Method::POST, &format!("/api/admin/users/{ada}/reset"), Some(&admin_cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["username"].clone(), v["hours"].clone()), (json!("ada"), json!(24)));
        let link = v["link"].as_str().unwrap();
        assert!(link.contains("/reset?token="), "{link}");
        let token = link.split("token=").nth(1).unwrap();

        // The link is the one the reset page takes: a new password, and the
        // old sessions end.
        let (status, _) = send(&app, Method::POST, "/api/register/reset", None, Some(json!({ "token": token, "password": "ett nytt lösenord" }))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = send(&app, Method::GET, "/api/account", Some(&ada_cookie), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = send(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "ada", "password": "ett nytt lösenord" }))).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send(&app, Method::POST, "/api/register/reset", None, Some(json!({ "token": token, "password": "ett tredje lösenord" }))).await;
        assert_eq!(status, StatusCode::GONE, "used once");

        // A link not used within the day stops working.
        let (_, v) = send(&app, Method::POST, &format!("/api/admin/users/{ada}/reset"), Some(&admin_cookie), None).await;
        let token = v["link"].as_str().unwrap().split("token=").nth(1).unwrap().to_string();
        sqlx::query("UPDATE users SET reset_expires = '2000-01-01T00:00:00.000Z' WHERE id = ?").bind(ada).execute(&db).await.unwrap();
        let (status, _) = send(&app, Method::POST, "/api/register/reset", None, Some(json!({ "token": token, "password": "ett tredje lösenord" }))).await;
        assert_eq!(status, StatusCode::GONE);

        // From the shell: a link for any user, named in any case.
        let (name, path) = super::reset_password_command(&db, "ADA").await.unwrap();
        assert_eq!(name, "ada");
        let token = path.split("token=").nth(1).unwrap();
        let (status, _) = send(&app, Method::POST, "/api/register/reset", None, Some(json!({ "token": token, "password": "ett tredje lösenord" }))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(super::reset_password_command(&db, "ingen").await.is_err());

        // Both are in the log, the one from the shell without an actor.
        let (_, log) = send(&app, Method::GET, "/api/admin/log?action=user.reset_link", Some(&admin_cookie), None).await;
        let entries = log["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0]["actor_name"].is_null());
        assert_eq!(entries[0]["details"]["from"], "command line");
        assert_eq!(entries[2]["actor_name"], "chef");
    }
}
