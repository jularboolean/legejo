//! Admin invitations. The admin creates the account (username, optional
//! email) and the invitee sets a password via a link valid for 7 days. The
//! link is mailed when an address is given and mail is configured; it is
//! always returned to the admin as well.
//!
//! Until accepted, the account has an unusable random password and
//! `invite_token` set, so it cannot log in and is listed as invited.

use crate::auth::{AdminUser, UserInfo, SESSION_DAYS};
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::json;

const INVITE_DAYS: i64 = 7;

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

fn gone() -> Response {
    err(StatusCode::GONE, "the link is invalid or has expired")
}

/// Base URL for links. LEGEJO_PUBLIC_URL wins when set, because behind a
/// reverse proxy X-Forwarded-Proto may be wrong.
pub(crate) fn base(state: &AppState, headers: &HeaderMap) -> String {
    match &state.fed.config {
        Some(c) => c.base.clone(),
        None => crate::register::base_url(headers),
    }
}

#[derive(Serialize)]
pub struct InviteResult {
    id: i64,
    username: String,
    link: String,
    /// The invitation went out by mail.
    mailed: bool,
    /// Why mailing failed, when an address was given.
    mail_error: Option<String>,
}

async fn send_invite(state: &AppState, email: &str, username: &str, inviter: &str, link: &str) -> Result<(), String> {
    let html = format!(
        "<p>Hello {username},</p>\
         <p>{inviter} has invited you to Legejo, an e-book library with OPDS and Kobo sync.</p>\
         <p>Choose a password to activate your account:</p>\
         <p><a href=\"{link}\">{link}</a></p>\
         <p>Your username is <strong>{username}</strong>. The link is valid for {INVITE_DAYS} days.</p>"
    );
    crate::register::send_mail(state, email, "You are invited to Legejo", &html)
        .await
        .map_err(|e| {
            tracing::warn!("invite mail failed: {e:#}");
            e.to_string()
        })
}

#[derive(Deserialize)]
pub struct CreateInvite {
    username: String,
    email: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Json(req): Json<CreateInvite>,
) -> Result<(StatusCode, Json<InviteResult>), Response> {
    let username = req.username.trim().to_string();
    if username.is_empty() || username.chars().count() > 60 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "username must be 1-60 characters"));
    }
    let email = req.email.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
    if let Some(e) = &email {
        if !crate::register::looks_like_email(e) {
            return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "that does not look like an email address"));
        }
    }
    let token = crate::auth::new_token();
    // An unusable password until the invitee picks one.
    let placeholder = crate::db::hash_password(&crate::auth::new_token()).map_err(internal)?;
    let id: i64 = match sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, is_admin, email, invite_token, invite_expires)
         VALUES ($1, $2, 0, $3, $4, $5) RETURNING id",
    )
    .bind(&username)
    .bind(&placeholder)
    .bind(&email)
    .bind(&token)
    .bind(crate::db::ts_in_days(INVITE_DAYS))
    .fetch_one(&state.db)
    .await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(err(StatusCode::CONFLICT, "username or email is already in use"));
        }
        Err(e) => return Err(internal(e)),
    };
    let link = format!("{}/invite?token={token}", base(&state, &headers));
    let (mailed, mail_error) = match &email {
        Some(e) if state.mail.is_some() => match send_invite(&state, e, &username, &admin.0.username, &link).await {
            Ok(()) => (true, None),
            Err(why) => (false, Some(why)),
        },
        _ => (false, None),
    };
    tracing::info!("invite: {} invited {username} (mailed: {mailed})", admin.0.username);
    crate::audit::log(&state, crate::audit::by(&admin.0), "user.invited", json!({ "user_id": id, "username": username, "mailed": mailed })).await;
    Ok((StatusCode::CREATED, Json(InviteResult { id, username, link, mailed, mail_error })))
}

/// Issue a new link (invalidating the old one) and mail it when possible.
pub async fn renew(
    State(state): State<AppState>,
    admin: AdminUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<InviteResult>, Response> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT username, email FROM users WHERE id = $1 AND invite_token IS NOT NULL")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(internal)?;
    let (username, email) = row.ok_or_else(|| err(StatusCode::NOT_FOUND, "no pending invite"))?;
    let token = crate::auth::new_token();
    sqlx::query("UPDATE users SET invite_token = $1, invite_expires = $2 WHERE id = $3")
        .bind(&token)
        .bind(crate::db::ts_in_days(INVITE_DAYS))
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    let link = format!("{}/invite?token={token}", base(&state, &headers));
    let (mailed, mail_error) = match &email {
        Some(e) if state.mail.is_some() => match send_invite(&state, e, &username, &admin.0.username, &link).await {
            Ok(()) => (true, None),
            Err(why) => (false, Some(why)),
        },
        _ => (false, None),
    };
    crate::audit::log(&state, crate::audit::by(&admin.0), "user.invite_renewed", json!({ "user_id": id, "username": username })).await;
    Ok(Json(InviteResult { id, username, link, mailed, mail_error }))
}

/// Withdraw a pending invite by deleting the pending account. Accepted
/// accounts are not affected.
pub async fn withdraw(State(state): State<AppState>, admin: AdminUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    let username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    let r = sqlx::query("DELETE FROM users WHERE id = $1 AND invite_token IS NOT NULL")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(err(StatusCode::NOT_FOUND, "no pending invite"));
    }
    crate::audit::log(&state, crate::audit::by(&admin.0), "user.invite_withdrawn", json!({ "user_id": id, "username": username })).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TokenParam {
    token: String,
}

/// Public: returns the invited username for the given token.
pub async fn lookup(State(state): State<AppState>, Query(q): Query<TokenParam>) -> Result<Json<serde_json::Value>, Response> {
    let username: Option<String> =
        sqlx::query_scalar("SELECT username FROM users WHERE invite_token = $1 AND invite_expires > $2")
            .bind(q.token.trim())
            .bind(now_ts())
            .fetch_optional(&state.db)
            .await
            .map_err(internal)?;
    let username = username.ok_or_else(gone)?;
    Ok(Json(json!({ "username": username })))
}

#[derive(Deserialize)]
pub struct AcceptInvite {
    token: String,
    password: String,
}

/// Public: set the password, consume the invite and start a session.
pub async fn accept(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AcceptInvite>,
) -> Result<(CookieJar, Json<UserInfo>), Response> {
    if req.password.len() < 8 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "password must be at least 8 characters"));
    }
    let row: Option<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, username, locale FROM users WHERE invite_token = $1 AND invite_expires > $2",
    )
    .bind(req.token.trim())
    .bind(now_ts())
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    let (id, username, locale) = row.ok_or_else(gone)?;
    let hash = crate::db::hash_password(&req.password).map_err(internal)?;
    // The admin supplied the address, so it counts as verified.
    sqlx::query(
        "UPDATE users SET password_hash = $1, invite_token = NULL, invite_expires = NULL,
                          verify_token = NULL, verify_expires = NULL
         WHERE id = $2",
    )
    .bind(&hash)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(internal)?;
    let token = crate::auth::new_token();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&token)
        .bind(id)
        .bind(crate::db::ts_in_days(SESSION_DAYS))
        .execute(&state.db)
        .await
        .map_err(internal)?;
    tracing::info!("invite: {username} accepted");
    crate::audit::log(&state, Some((id, &username)), "user.invite_accepted", json!({})).await;
    let info = UserInfo { id, username, is_admin: false, locale, has_avatar: false };
    Ok((jar.add(crate::auth::session_cookie(token, SESSION_DAYS, state.settings.secure_cookies)), Json(info)))
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    #[tokio::test]
    async fn invite_flow() {
        let (app, db, _) = test_app().await;
        let (admin, admin_cookie) = add_user(&db, "chef").await;
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin).execute(&db).await.unwrap();
        let (_, plain_cookie) = add_user(&db, "vanlig").await;

        // Only admins invite.
        let (status, _) = send(&app, Method::POST, "/api/admin/invites", Some(&plain_cookie), Some(json!({ "username": "x" }))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, v) = send(&app, Method::POST, "/api/admin/invites", Some(&admin_cookie), Some(json!({ "username": "Carol" }))).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        assert_eq!(v["mailed"], false);
        let link = v["link"].as_str().unwrap().to_string();
        let token = link.split("token=").nth(1).unwrap().to_string();
        let id = v["id"].as_i64().unwrap();

        // Taken names are refused; the invitee is listed as invited.
        let (status, _) = send(&app, Method::POST, "/api/admin/invites", Some(&admin_cookie), Some(json!({ "username": "carol" }))).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (_, users) = send(&app, Method::GET, "/api/admin/users", Some(&admin_cookie), None).await;
        let carol = users.as_array().unwrap().iter().find(|u| u["username"] == "Carol").unwrap().clone();
        assert_eq!(carol["invited"], true);

        let (status, v) = send(&app, Method::GET, &format!("/api/invite?token={token}"), None, None).await;
        assert_eq!((status, v["username"].clone()), (StatusCode::OK, json!("Carol")));

        // A renewed link replaces the old one.
        let (_, v) = send(&app, Method::POST, &format!("/api/admin/invites/{id}/renew"), Some(&admin_cookie), None).await;
        let token2 = v["link"].as_str().unwrap().split("token=").nth(1).unwrap().to_string();
        let (status, _) = send(&app, Method::GET, &format!("/api/invite?token={token}"), None, None).await;
        assert_eq!(status, StatusCode::GONE);

        let (status, _) = send(&app, Method::POST, "/api/invite", None, Some(json!({ "token": token2, "password": "kort" }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, v) = send(&app, Method::POST, "/api/invite", None, Some(json!({ "token": token2, "password": "ett bra lösenord" }))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["username"], "Carol");

        // Used once; then a normal login works.
        let (status, _) = send(&app, Method::POST, "/api/invite", None, Some(json!({ "token": token2, "password": "ett bra lösenord" }))).await;
        assert_eq!(status, StatusCode::GONE);
        let (status, _) = send(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "carol", "password": "ett bra lösenord" }))).await;
        assert_eq!(status, StatusCode::OK);

        // Grant and revoke admin; revoking one's own rights is refused.
        let carol_id = id;
        let (status, _) = send(&app, Method::PUT, &format!("/api/admin/users/{carol_id}/admin"), Some(&admin_cookie), Some(json!({ "is_admin": true }))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let flag: i64 = sqlx::query_scalar("SELECT is_admin FROM users WHERE id = ?").bind(carol_id).fetch_one(&db).await.unwrap();
        assert_eq!(flag, 1);
        let (status, _) = send(&app, Method::PUT, &format!("/api/admin/users/{carol_id}/admin"), Some(&admin_cookie), Some(json!({ "is_admin": false }))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (status, _) = send(&app, Method::PUT, &format!("/api/admin/users/{admin}/admin"), Some(&admin_cookie), Some(json!({ "is_admin": false }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = send(&app, Method::PUT, &format!("/api/admin/users/{carol_id}/admin"), Some(&plain_cookie), Some(json!({ "is_admin": true }))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // All of the above is in the audit log, newest first.
        let (status, log) = send(&app, Method::GET, "/api/admin/log", Some(&admin_cookie), None).await;
        assert_eq!(status, StatusCode::OK);
        let actions: Vec<&str> = log["entries"].as_array().unwrap().iter().map(|e| e["action"].as_str().unwrap()).collect();
        assert_eq!(actions, ["user.admin_revoked", "user.admin_granted", "user.invite_accepted", "user.invite_renewed", "user.invited"]);
        assert_eq!(log["entries"][4]["details"]["username"], "Carol");
        assert_eq!(log["entries"][4]["actor_name"], "chef");
        let (_, only) = send(&app, Method::GET, "/api/admin/log?action=user.invited", Some(&admin_cookie), None).await;
        assert_eq!(only["entries"].as_array().unwrap().len(), 1);
        let (_, page) = send(&app, Method::GET, "/api/admin/log?limit=2", Some(&admin_cookie), None).await;
        assert_eq!(page["entries"].as_array().unwrap().len(), 2);
        let next = page["next"].as_i64().unwrap();
        let (_, rest) = send(&app, Method::GET, &format!("/api/admin/log?limit=10&before={next}"), Some(&admin_cookie), None).await;
        assert_eq!(rest["entries"].as_array().unwrap().len(), 3);
        assert!(rest["next"].is_null());
        let (status, _) = send(&app, Method::GET, "/api/admin/log", Some(&plain_cookie), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // An accepted account can't be "withdrawn".
        let (status, _) = send(&app, Method::DELETE, &format!("/api/admin/invites/{id}"), Some(&admin_cookie), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
