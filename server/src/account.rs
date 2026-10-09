use crate::auth::{self, AuthUser};
use crate::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub struct Account {
    pub username: String,
    pub kobo_token: Option<String>,
    /// Where Send to Kindle mails the user's books.
    pub kindle_email: Option<String>,
    /// The address those mails come from, which the user has to approve
    /// with Amazon. None when the server sends no mail.
    #[sqlx(skip)]
    pub mail_from: Option<String>,
    /// Whether the user has turned the librarian on (librarian.rs).
    pub librarian: crate::db::DbFlag,
    /// Whether there is a librarian to turn on: the operator has set a
    /// language model up.
    #[sqlx(skip)]
    pub librarian_available: bool,
    /// Whether the user has turned the fediverse on: following shelves on
    /// other instances.
    pub fediverse: crate::db::DbFlag,
    /// Whether there is a fediverse to turn on: the instance federates.
    #[sqlx(skip)]
    pub fediverse_available: bool,
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

fn unprocessable(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response()
}

async fn fetch_account(state: &AppState, user_id: i64) -> Result<Account, Response> {
    let mut account: Account = sqlx::query_as("SELECT username, kobo_token, kindle_email, librarian, fediverse FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    account.mail_from = state.mail.as_ref().and_then(|m| m.from_address());
    account.librarian_available = state.settings.ai.is_some();
    account.fediverse_available = crate::fed::active(state).await.is_some();
    Ok(account)
}

#[derive(Deserialize)]
pub struct UpdateFediverse {
    enabled: bool,
}

/// Turn the fediverse on or off for oneself. Off only takes the page for
/// following shelves out of the way: shelves one already federates stay
/// federated, and what one follows stays followed.
pub async fn set_fediverse(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateFediverse>,
) -> Result<Json<Account>, Response> {
    sqlx::query("UPDATE users SET fediverse = $1 WHERE id = $2")
        .bind(crate::db::DbFlag::from(req.enabled))
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateLibrarian {
    enabled: bool,
}

/// Turn the librarian on or off for oneself. On means that what the
/// catalogue says about one's books is sent to the operator's language model
/// when one asks it something.
pub async fn set_librarian(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateLibrarian>,
) -> Result<Json<Account>, Response> {
    sqlx::query("UPDATE users SET librarian = $1 WHERE id = $2")
        .bind(crate::db::DbFlag::from(req.enabled))
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateKindle {
    /// Empty or absent removes the address.
    email: Option<String>,
}

/// Amazon's Send to Kindle addresses. Keeping to them means the server
/// cannot be used to mail book files to arbitrary addresses.
fn kindle_address(email: &str) -> bool {
    let email = email.to_lowercase();
    crate::register::looks_like_email(&email)
        && ["@kindle.com", "@free.kindle.com", "@kindle.cn"].iter().any(|domain| email.ends_with(domain))
}

pub async fn set_kindle(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateKindle>,
) -> Result<Json<Account>, Response> {
    let email = req.email.map(|e| e.trim().to_string()).filter(|e| !e.is_empty());
    if email.as_deref().is_some_and(|e| !kindle_address(e)) {
        return Err(unprocessable("not-kindle"));
    }
    sqlx::query("UPDATE users SET kindle_email = $1 WHERE id = $2")
        .bind(email.as_deref())
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

pub async fn get(State(state): State<AppState>, user: AuthUser) -> Result<Json<Account>, Response> {
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateAccount {
    current_password: String,
    username: String,
    new_password: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    jar: CookieJar,
    Json(req): Json<UpdateAccount>,
) -> Result<Json<Account>, Response> {
    let username = req.username.trim();
    if username.is_empty() {
        return Err(unprocessable("username must not be empty"));
    }
    let new_password = req.new_password.filter(|p| !p.is_empty());
    if new_password.as_deref().is_some_and(|p| p.len() < 8) {
        return Err(unprocessable("password must be at least 8 characters"));
    }

    // Any account change requires the current password.
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let parsed = PasswordHash::new(&hash).map_err(|e| internal(anyhow::anyhow!("{e}")))?;
    if Argon2::default().verify_password(req.current_password.as_bytes(), &parsed).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "wrong current password" })),
        )
            .into_response());
    }

    let result = sqlx::query("UPDATE users SET username = $1 WHERE id = $2")
        .bind(username)
        .bind(user.0.id)
        .execute(&state.db)
        .await;
    match result {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "that username is taken" })),
            )
                .into_response());
        }
        Err(e) => return Err(internal(e.into())),
    }

    let password_changed = new_password.is_some();
    if let Some(password) = new_password {
        let new_hash = crate::db::hash_password(&password).map_err(internal)?;
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(&new_hash)
            .bind(user.0.id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;

        // Log out every other session, but keep the one making the change.
        let current = jar.get(auth::SESSION_COOKIE).map(|c| c.value().to_string()).unwrap_or_default();
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND token != $2")
            .bind(user.0.id)
            .bind(&current)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    }

    crate::audit::log(
        &state,
        Some((user.0.id, username)),
        "account.updated",
        serde_json::json!({ "renamed_from": (username != user.0.username).then_some(&user.0.username), "password_changed": password_changed }),
    )
    .await;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;

fn avatar_path(state: &AppState, user_id: i64) -> std::path::PathBuf {
    state.data_dir.join("avatars").join(user_id.to_string())
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<StatusCode, Response> {
    let mut uploaded: Option<(Vec<u8>, String)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let mime = field.content_type().unwrap_or("").to_string();
        if !mime.starts_with("image/") {
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| internal(e.into()))?;
        if bytes.len() > MAX_AVATAR_BYTES {
            return Err(unprocessable("image too large (max 5 MB)"));
        }
        uploaded = Some((bytes.to_vec(), mime));
        break;
    }
    let (bytes, mime) = uploaded.ok_or_else(|| unprocessable("no image file in upload"))?;

    tokio::fs::create_dir_all(state.data_dir.join("avatars"))
        .await
        .map_err(|e| internal(e.into()))?;
    tokio::fs::write(avatar_path(&state, user.0.id), &bytes)
        .await
        .map_err(|e| internal(e.into()))?;
    sqlx::query("UPDATE users SET avatar_mime = $1 WHERE id = $2")
        .bind(&mime)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_avatar(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<StatusCode, Response> {
    sqlx::query("UPDATE users SET avatar_mime = NULL WHERE id = $1")
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let _ = tokio::fs::remove_file(avatar_path(&state, user.0.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Any logged-in user may see any user's picture (shown on public shelves).
/// 404 when none is uploaded; the frontend then shows the default.
pub async fn avatar(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let mime: Option<Option<String>> =
        sqlx::query_scalar("SELECT avatar_mime FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    let mime = mime.flatten().ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response()
    })?;
    let data = tokio::fs::read(avatar_path(&state, id)).await.map_err(|_| {
        (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response()
    })?;
    Ok((
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, no-cache".into())],
        data,
    )
        .into_response())
}

const LOCALES: [&str; 7] = ["en", "sv", "fi", "eo", "fr", "de", "es"];

#[derive(Deserialize)]
pub struct UpdateLocale {
    locale: String,
}

/// Language is cosmetic, so unlike username/password it needs no password.
pub async fn set_locale(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateLocale>,
) -> Result<StatusCode, Response> {
    if !LOCALES.contains(&req.locale.as_str()) {
        return Err(unprocessable("unknown language"));
    }
    sqlx::query("UPDATE users SET locale = $1 WHERE id = $2")
        .bind(&req.locale)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_kobo_token(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Account>, Response> {
    let token = auth::new_token();
    sqlx::query("UPDATE users SET kobo_token = $1 WHERE id = $2")
        .bind(&token)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.kobo_token_created", serde_json::json!({})).await;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

pub async fn delete_kobo_token(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Account>, Response> {
    sqlx::query("UPDATE users SET kobo_token = NULL WHERE id = $1")
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.kobo_token_removed", serde_json::json!({})).await;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

#[derive(Deserialize)]
pub struct DeleteAccount {
    password: String,
}

/// DELETE /api/account: removes the account with its books (including
/// files), shelves, reading positions, exports and avatar. Requires the
/// current password. The last admin cannot delete their account.
pub async fn delete_account(
    State(state): State<AppState>,
    user: AuthUser,
    jar: CookieJar,
    Json(req): Json<DeleteAccount>,
) -> Result<CookieJar, Response> {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let parsed = PasswordHash::new(&hash).map_err(|e| internal(anyhow::anyhow!("{e}")))?;
    if Argon2::default().verify_password(req.password.as_bytes(), &parsed).is_err() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "wrong current password" })),
        )
            .into_response());
    }
    if user.0.is_admin {
        let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = 1")
            .fetch_one(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        if admins <= 1 {
            return Err(unprocessable("the last admin cannot delete their account"));
        }
    }

    // Retract federated shelves so followers drop them.
    if let Some(c) = crate::fed::active(&state).await {
        let federated: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, ap_slug FROM shelves WHERE owner_id = $1 AND visibility = 'federated' AND ap_slug IS NOT NULL",
        )
        .bind(user.0.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        for (id, slug) in federated {
            if let Err(e) = crate::fed::reconcile::retract_shelf(&state, &c, id, &slug).await {
                tracing::warn!("account deletion: could not retract {slug}: {e:#}");
            }
        }
    }

    let books: Vec<(i64, String)> = sqlx::query_as("SELECT id, uuid FROM books WHERE owner_id = $1")
        .bind(user.0.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for (id, uuid) in &books {
        crate::books::remove_book(&state, user.0.id, *id, uuid).await?;
    }
    let audiobooks: Vec<String> = sqlx::query_scalar("SELECT uuid FROM audiobooks WHERE owner_id = $1")
        .bind(user.0.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for uuid in &audiobooks {
        crate::audiobooks::remove_files(&state, uuid).await;
    }
    let shelves: Vec<i64> = sqlx::query_scalar("SELECT id FROM shelves WHERE owner_id = $1")
        .bind(user.0.id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for id in shelves {
        let _ = tokio::fs::remove_file(state.data_dir.join("shelf_covers").join(id.to_string())).await;
    }
    let _ = tokio::fs::remove_file(avatar_path(&state, user.0.id)).await;
    let _ = tokio::fs::remove_dir_all(state.data_dir.join("exports").join(user.0.id.to_string())).await;

    // Everything else (shelves, sessions, progress, follows, exports) cascades.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    state.fed.wake.notify_one();
    crate::audit::log(
        &state,
        Some((user.0.id, user.0.username.as_str())),
        "user.deleted",
        serde_json::json!({ "username": user.0.username, "books": books.len() }),
    )
    .await;
    tracing::info!("account {} deleted ({} books)", user.0.username, books.len());
    let mut removal = axum_extra::extract::cookie::Cookie::from(auth::SESSION_COOKIE);
    removal.set_path("/");
    Ok(jar.remove(removal))
}
