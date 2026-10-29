//! First start in the browser. A new instance with no accounts, and no
//! LEGEJO_ADMIN_PASSWORD to make one from, asks the first visitor to create
//! the admin account: no password to fish out of the log. Once an account
//! exists, the way is closed for good.

use crate::auth::{session_cookie, UserInfo, SESSION_DAYS};
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::json;

/// Whether the instance still waits for its first account.
pub async fn needed(state: &AppState) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users").fetch_one(&state.db).await.is_ok_and(|n| n == 0)
}

#[derive(Deserialize)]
pub struct Setup {
    username: String,
    password: String,
    /// The language the visitor chose on the page, for the new account.
    #[serde(default)]
    locale: Option<String>,
}

/// Two first visitors at once must not both become admin.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Public, and only while there is no account: create the admin and log in.
pub async fn create(State(state): State<AppState>, jar: CookieJar, Json(req): Json<Setup>) -> Result<(CookieJar, Json<UserInfo>), Response> {
    let username = crate::auth::validate_username(&req.username).map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))?.to_string();
    if req.password.len() < 8 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "password must be at least 8 characters"));
    }
    let locale = req.locale.filter(|l| crate::account::LOCALES.contains(&l.as_str()));
    let hash = crate::db::hash_password(&req.password).await.map_err(internal)?;

    let _turn = ONE_AT_A_TIME.lock().await;
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, is_admin, locale)
         SELECT $1, $2, 1, $3 WHERE NOT EXISTS (SELECT 1 FROM users)
         RETURNING id",
    )
    .bind(&username)
    .bind(&hash)
    .bind(&locale)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let Some(id) = id else {
        return Err(err(StatusCode::CONFLICT, "this Legejo already has an account"));
    };
    tracing::info!("first start: admin account '{username}' created in the browser");
    // A new instance: its library begins with the manual.
    crate::welcome::shelve(&state, id).await;

    let token = crate::auth::new_token();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&token)
        .bind(id)
        .bind(crate::db::ts_in_days(SESSION_DAYS))
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let info = UserInfo { id, username, is_admin: true, locale, has_avatar: false };
    Ok((jar.add(session_cookie(token, SESSION_DAYS, crate::auth::secure_cookies(&state))), Json(info)))
}

fn err(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}
