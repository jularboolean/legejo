use crate::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::{FromRequestParts, State};
use axum::http::{request::Parts, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub(crate) const SESSION_COOKIE: &str = "legejo_session";
pub(crate) const SESSION_DAYS: i64 = 30;

#[derive(Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    /// UI language code; None means English.
    pub locale: Option<String>,
    pub has_avatar: bool,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
    password_hash: String,
    is_admin: i64,
    locale: Option<String>,
    has_avatar: i64,
    verify_token: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

pub struct AuthUser(pub UserInfo);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| unauthorized())?;
        let token = jar.get(SESSION_COOKIE).ok_or_else(unauthorized)?.value().to_string();

        let row: Option<(i64, String, i64, Option<String>, i64)> = sqlx::query_as(
            "SELECT u.id, u.username, u.is_admin, u.locale,
                    CAST(CASE WHEN u.avatar_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT)
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token = $1 AND s.expires_at > $2",
        )
        .bind(&token)
        .bind(crate::db::now_ts())
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;

        match row {
            Some((id, username, is_admin, locale, has_avatar)) => Ok(AuthUser(UserInfo {
                id,
                username,
                is_admin: is_admin != 0,
                locale,
                has_avatar: has_avatar != 0,
            })),
            None => Err(unauthorized()),
        }
    }
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "not logged in" }))).into_response()
}

/// Like AuthUser, but the session must belong to an admin; 403 otherwise.
/// The payload is currently unused; handlers only need the gate.
pub struct AdminUser(#[allow(dead_code)] pub UserInfo);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "admin only" })),
            )
                .into_response());
        }
        Ok(AdminUser(user))
    }
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

pub(crate) fn session_cookie(token: String, max_age_days: i64, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(SESSION_COOKIE, token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::days(max_age_days));
    cookie
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserInfo>), Response> {
    let user: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, password_hash, is_admin, locale, \
                CAST(CASE WHEN avatar_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_avatar, \
                verify_token \
         FROM users WHERE LOWER(username) = LOWER($1)",
    )
    .bind(req.username.trim())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let user = user.ok_or_else(invalid_credentials)?;

    let parsed = PasswordHash::new(&user.password_hash).map_err(|e| internal(anyhow::anyhow!("{e}")))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed)
        .map_err(|_| invalid_credentials())?;

    if user.verify_token.is_some() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "email not verified" })),
        )
            .into_response());
    }

    let token = new_token();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&token)
        .bind(user.id)
        .bind(crate::db::ts_in_days(SESSION_DAYS))
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;

    let info = UserInfo {
        id: user.id,
        username: user.username,
        is_admin: user.is_admin != 0,
        locale: user.locale,
        has_avatar: user.has_avatar != 0,
    };
    Ok((jar.add(session_cookie(token, SESSION_DAYS, state.settings.secure_cookies)), Json(info)))
}

fn invalid_credentials() -> Response {
    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid username or password" })))
        .into_response()
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<CookieJar, Response> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE token = $1")
            .bind(cookie.value())
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    }
    let mut removal = Cookie::from(SESSION_COOKIE);
    removal.set_path("/");
    Ok(jar.remove(removal))
}

pub async fn me(user: AuthUser) -> Json<UserInfo> {
    Json(user.0)
}

pub(crate) fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
