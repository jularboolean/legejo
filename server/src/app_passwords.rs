//! Per-app passwords for OPDS. Each reader app gets its own revocable random
//! secret, so the account password never has to be entered into an app.
//! Accounts without a password (OIDC logins) need one to use OPDS at all.
//!
//! The account password still works for OPDS. App passwords are checked first
//! because the check is cheap: the secret is 16 random characters from a
//! 31-letter alphabet (about 79 bits), so a SHA-256 lookup suffices where a
//! human-chosen password would need Argon2.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn hash(secret: &str) -> String {
    // Ignore hyphens and case, which are awkward to type on an e-reader.
    let normalized: String = secret.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    crate::books::sha256_hex(normalized.as_bytes())
}

/// "abcd-efgh-jkmn-pqrs": easy to read and type, no 0/o, 1/l/i.
fn generate() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    let raw: Vec<char> = (0..16).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect();
    raw.chunks(4).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-")
}

/// True when `secret` is one of the user's app passwords. Updates
/// `last_used_at` at most once an hour to avoid a write per OPDS request.
pub async fn matches(state: &AppState, user_id: i64, secret: &str) -> bool {
    if secret.len() < 16 || secret.len() > 40 {
        return false;
    }
    let row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, last_used_at FROM app_passwords WHERE user_id = $1 AND secret_hash = $2")
            .bind(user_id)
            .bind(hash(secret))
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let Some((id, last)) = row else { return false };
    let hour_ago = (time::OffsetDateTime::now_utc() - time::Duration::hours(1))
        .format(time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"))
        .unwrap_or_default();
    if last.is_none_or(|l| l < hour_ago) {
        let _ = sqlx::query("UPDATE app_passwords SET last_used_at = $1 WHERE id = $2").bind(now_ts()).bind(id).execute(&state.db).await;
    }
    true
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AppPassword {
    id: i64,
    name: String,
    created_at: String,
    last_used_at: Option<String>,
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<AppPassword>>, Response> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, created_at, last_used_at FROM app_passwords WHERE user_id = $1 ORDER BY id")
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
            .map_err(internal)?,
    ))
}

#[derive(Deserialize)]
pub struct Create {
    name: String,
}

/// POST /api/account/app-passwords. The secret is only returned here.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<Create>,
) -> Result<(StatusCode, Json<serde_json::Value>), Response> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "name must be 1-60 characters" }))).into_response());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_passwords WHERE user_id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if count >= 50 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "too many app passwords" }))).into_response());
    }
    let secret = generate();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO app_passwords (user_id, name, secret_hash, created_at) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user.0.id)
    .bind(name)
    .bind(hash(&secret))
    .bind(now_ts())
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.app_password_created", json!({ "name": name })).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "name": name, "secret": secret }))))
}

pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM app_passwords WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    let Some(name) = name else {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response());
    };
    sqlx::query("DELETE FROM app_passwords WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.app_password_removed", json!({ "name": name })).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_typeable_and_forgiving() {
        let s = generate();
        assert_eq!(s.len(), 19);
        assert_eq!(s.matches('-').count(), 3);
        assert_eq!(hash(&s), hash(&s.replace('-', "").to_uppercase()));
        assert_ne!(hash(&s), hash(&generate()));
    }
}
