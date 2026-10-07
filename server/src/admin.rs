//! Admin endpoints (instance settings, user list, admin rights, test mail)
//! and the read-only /api/config available to every logged-in user.

use crate::auth::{AdminUser, AuthUser};
use crate::db::DbFlag;
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

/// Read a boolean setting; an absent key means the given default.
pub async fn setting_bool(state: &AppState, key: &str, default: bool) -> Result<bool, sqlx::Error> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(&state.db)
        .await?;
    Ok(value.map(|v| v == "true").unwrap_or(default))
}

async fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub async fn libris_enabled(state: &AppState) -> Result<bool, sqlx::Error> {
    setting_bool(state, "libris_enabled", true).await
}

#[derive(Serialize)]
pub struct Settings {
    libris_enabled: bool,
    openlibrary_enabled: bool,
    audiobooks_enabled: bool,
    registration_enabled: bool,
    /// Whether the operator configured SMTP (LEGEJO_SMTP_*); read-only info.
    mail_configured: bool,
}

async fn current_settings(state: &AppState) -> Result<Json<Settings>, Response> {
    Ok(Json(Settings {
        libris_enabled: libris_enabled(state).await.map_err(|e| internal(e.into()))?,
        openlibrary_enabled: crate::openlibrary::enabled(state).await,
        audiobooks_enabled: crate::audiobooks::enabled(state).await,
        registration_enabled: crate::register::registration_enabled(state)
            .await
            .map_err(|e| internal(e.into()))?,
        mail_configured: state.mail.is_some(),
    }))
}

pub async fn get_settings(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Settings>, Response> {
    current_settings(&state).await
}

#[derive(Deserialize)]
pub struct UpdateSettings {
    libris_enabled: bool,
    /// Absent from older clients: unchanged.
    openlibrary_enabled: Option<bool>,
    /// Absent from older clients: unchanged.
    audiobooks_enabled: Option<bool>,
    registration_enabled: bool,
}

pub async fn update_settings(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<UpdateSettings>,
) -> Result<Json<Settings>, Response> {
    set_setting(&state, "libris_enabled", if req.libris_enabled { "true" } else { "false" })
        .await
        .map_err(|e| internal(e.into()))?;
    if let Some(on) = req.openlibrary_enabled {
        set_setting(&state, "openlibrary_enabled", if on { "true" } else { "false" })
            .await
            .map_err(|e| internal(e.into()))?;
    }
    if let Some(on) = req.audiobooks_enabled {
        set_setting(&state, "audiobooks_enabled", if on { "true" } else { "false" })
            .await
            .map_err(|e| internal(e.into()))?;
    }
    set_setting(
        &state,
        "registration_enabled",
        if req.registration_enabled { "true" } else { "false" },
    )
    .await
    .map_err(|e| internal(e.into()))?;
    crate::audit::log(
        &state,
        crate::audit::by(&admin.0),
        "admin.settings",
        serde_json::json!({
            "libris_enabled": req.libris_enabled,
            "openlibrary_enabled": req.openlibrary_enabled,
            "audiobooks_enabled": req.audiobooks_enabled,
            "registration_enabled": req.registration_enabled,
        }),
    )
    .await;
    current_settings(&state).await
}

#[derive(Deserialize)]
pub struct TestMail {
    to: String,
}

/// Lets the admin prove the mail settings work before enabling registration.
pub async fn test_mail(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<TestMail>,
) -> Result<StatusCode, Response> {
    crate::register::send_mail(
        &state,
        req.to.trim(),
        "Legejo test mail",
        "<p>The mail settings work. Greetings from Legejo!</p>",
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": format!("{e}") })),
        )
            .into_response()
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserRow {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub is_admin: DbFlag,
    pub verified: DbFlag,
    /// Invited by an admin, password not chosen yet.
    pub invited: DbFlag,
    pub has_avatar: DbFlag,
    pub created_at: String,
    pub book_count: i64,
    pub shelf_count: i64,
}

pub async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<UserRow>>, Response> {
    let users: Vec<UserRow> = sqlx::query_as(
        "SELECT u.id, u.username, u.email, u.is_admin,
                CAST(CASE WHEN u.verify_token IS NULL THEN 1 ELSE 0 END AS BIGINT) AS verified,
                CAST(CASE WHEN u.invite_token IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS invited,
                CAST(CASE WHEN u.avatar_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_avatar,
                u.created_at, COUNT(b.id) AS book_count,
                (SELECT COUNT(*) FROM shelves s WHERE s.owner_id = u.id) AS shelf_count
         FROM users u
         LEFT JOIN books b ON b.owner_id = u.id
         GROUP BY u.id
         ORDER BY LOWER(u.username)",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(users))
}

#[derive(Deserialize)]
pub struct SetAdmin {
    is_admin: bool,
}

/// Grant or revoke admin. Admins cannot revoke their own rights, which
/// guarantees at least one admin remains.
pub async fn set_admin(
    State(state): State<AppState>,
    admin: AdminUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<SetAdmin>,
) -> Result<StatusCode, Response> {
    if id == admin.0.id && !req.is_admin {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "you cannot remove your own admin rights" })),
        )
            .into_response());
    }
    let r = sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2")
        .bind(DbFlag::from(req.is_admin))
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if r.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response());
    }
    tracing::info!("admin: {} set is_admin={} for user {id}", admin.0.username, req.is_admin);
    let name: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    crate::audit::log(
        &state,
        crate::audit::by(&admin.0),
        if req.is_admin { "user.admin_granted" } else { "user.admin_revoked" },
        serde_json::json!({ "user_id": id, "username": name }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// What every logged-in user may know about the instance; the frontend uses
/// it to show or hide features.
#[derive(Serialize)]
pub struct Config {
    libris_enabled: bool,
    openlibrary_enabled: bool,
    /// Whether the MCP endpoint is served (LEGEJO_MCP).
    mcp_enabled: bool,
    audiobooks_enabled: bool,
    /// Whether the user can mail books to a Kindle: the server sends mail
    /// and the user has given their Kindle's address.
    send_to_kindle: bool,
}

pub async fn config(State(state): State<AppState>, user: AuthUser) -> Result<Json<Config>, Response> {
    let kindle: Option<String> = sqlx::query_scalar("SELECT kindle_email FROM users WHERE id = $1")
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?
        .flatten();
    Ok(Json(Config {
        send_to_kindle: state.mail.is_some() && kindle.is_some(),
        libris_enabled: libris_enabled(&state).await.map_err(|e| internal(e.into()))?,
        openlibrary_enabled: crate::openlibrary::enabled(&state).await,
        mcp_enabled: state.settings.mcp,
        audiobooks_enabled: crate::audiobooks::enabled(&state).await,
    }))
}
