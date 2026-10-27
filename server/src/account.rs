use crate::auth::{self, AuthUser};
use crate::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub struct Account {
    pub username: String,
    /// The address for password resets and sign-in by email.
    pub email: Option<String>,
    /// A new address waiting to be confirmed from a link sent to it.
    pub pending_email: Option<String>,
    /// Whether the server sends mail: a new address is then confirmed
    /// before it is used.
    #[sqlx(skip)]
    pub mail_configured: bool,
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
    /// Whether the user has turned on the catalogs of other libraries
    /// (catalogs.rs).
    pub catalogs: crate::db::DbFlag,
    /// Whether there are catalogs to turn on: an admin has turned them on
    /// for the instance.
    #[sqlx(skip)]
    pub catalogs_available: bool,
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

fn unprocessable(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response()
}

async fn fetch_account(state: &AppState, user_id: i64) -> Result<Account, Response> {
    let mut account: Account = sqlx::query_as(
        "SELECT username, email, pending_email, kobo_token, kindle_email, librarian, fediverse, catalogs FROM users WHERE id = $1",
    )
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    account.mail_from = state.live.mail().as_ref().and_then(|m| m.from_address());
    account.mail_configured = state.live.mail().is_some();
    account.librarian_available = state.live.ai().is_some();
    account.fediverse_available = crate::fed::active(state).await.is_some();
    account.catalogs_available = crate::catalogs::enabled(state).await;
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
pub struct UpdateCatalogs {
    enabled: bool,
}

/// Turn the catalogs of other libraries on or off for oneself. Off takes
/// the page out of the way; the catalogs one has added are kept, and so are
/// the books fetched from them.
pub async fn set_catalogs(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<UpdateCatalogs>,
) -> Result<Json<Account>, Response> {
    sqlx::query("UPDATE users SET catalogs = $1 WHERE id = $2")
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
    /// The email address wanted; empty removes it. Absent from older
    /// clients: unchanged.
    email: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    jar: CookieJar,
    headers: axum::http::HeaderMap,
    Json(req): Json<UpdateAccount>,
) -> Result<Json<Account>, Response> {
    let username = crate::auth::validate_username(&req.username).map_err(unprocessable)?;
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
    if !crate::db::verify_password(&hash, &req.current_password).await.map_err(internal)? {
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

    let email_change = match &req.email {
        Some(wanted) => change_email(&state, &headers, user.0.id, username, wanted).await?,
        None => None,
    };

    let password_changed = new_password.is_some();
    if let Some(password) = new_password {
        let new_hash = crate::db::hash_password(&password).await.map_err(internal)?;
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
        serde_json::json!({
            "renamed_from": (username != user.0.username).then_some(&user.0.username),
            "password_changed": password_changed,
            "email": email_change,
        }),
    )
    .await;
    Ok(Json(fetch_account(&state, user.0.id).await?))
}

/// The email address the user asked for, after the current password was
/// checked: "removed", "confirm" (a link went to the new address, which
/// is used once it is opened), "set", or None when nothing changed.
///
/// Without mail an address cannot be confirmed, and it is set at once;
/// except with single sign-on, which joins accounts by email: an address
/// nobody confirmed could then join someone else's sign-in to this account.
async fn change_email(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    user_id: i64,
    username: &str,
    wanted: &str,
) -> Result<Option<&'static str>, Response> {
    let wanted = wanted.trim();
    let (current, pending): (Option<String>, Option<String>) = sqlx::query_as("SELECT email, pending_email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if wanted.is_empty() {
        if current.is_none() && pending.is_none() {
            return Ok(None);
        }
        sqlx::query("UPDATE users SET email = NULL, pending_email = NULL, email_token = NULL, email_token_expires = NULL WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        return Ok(Some("removed"));
    }
    let same = |a: &Option<String>| a.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(wanted));
    if same(&current) {
        // Back to the address in use: a pending change is dropped.
        if pending.is_some() {
            sqlx::query("UPDATE users SET pending_email = NULL, email_token = NULL, email_token_expires = NULL WHERE id = $1")
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
        }
        return Ok(None);
    }
    if !crate::register::looks_like_email(wanted) {
        return Err(unprocessable("not an email address"));
    }
    let taken: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id != $1 AND email IS NOT NULL AND LOWER(email) = LOWER($2)")
        .bind(user_id)
        .bind(wanted)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if taken > 0 {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "that email is in use" }))).into_response());
    }
    if state.live.mail().is_none() {
        if state.oidc.is_some() {
            return Err(unprocessable("email cannot be confirmed without mail"));
        }
        let set = sqlx::query("UPDATE users SET email = $1, pending_email = NULL, email_token = NULL, email_token_expires = NULL WHERE id = $2")
            .bind(wanted)
            .bind(user_id)
            .execute(&state.db)
            .await;
        return match set {
            Ok(_) => Ok(Some("set")),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "that email is in use" }))).into_response())
            }
            Err(e) => Err(internal(e.into())),
        };
    }
    // Asked again, for the same address or another: a fresh link, within
    // the rations for mail to one address.
    if !crate::ratelimit::mail_allowed(state, headers, wanted).await {
        return Err(crate::ratelimit::too_many(state, true));
    }
    let token = crate::auth::new_token();
    sqlx::query("UPDATE users SET pending_email = $1, email_token = $2, email_token_expires = $3 WHERE id = $4")
        .bind(wanted)
        .bind(&token)
        .bind(crate::db::ts_in_days(2))
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let link = format!("{}/confirm-email?token={token}", crate::invite::base(state, headers));
    let name = crate::booktext::esc(username);
    let html = format!(
        "<p>Hello {name},</p>\
         <p>Confirm that this is the new email address of your Legejo account:</p>\
         <p><a href=\"{link}\">{link}</a></p>\
         <p>The link is valid for 48 hours. Until it is opened, the account keeps its old address. \
         If you did not ask for this, ignore this mail.</p>",
    );
    if let Err(e) = crate::register::send_mail(state, wanted, "Confirm your new email address", &html).await {
        tracing::warn!("email change mail failed: {e:#}");
        let _ = sqlx::query("UPDATE users SET pending_email = NULL, email_token = NULL, email_token_expires = NULL WHERE id = $1")
            .bind(user_id)
            .execute(&state.db)
            .await;
        return Err((StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": "could not send the confirmation email" }))).into_response());
    }
    // The old address hears of it, so that a change nobody asked for shows.
    if let Some(old) = current {
        let notice = format!(
            "<p>Hello {name},</p>\
             <p>Someone asked to change the email address of your Legejo account to {new}. \
             It changes only when the link sent there is opened. If this was not you, \
             change your password.</p>",
            new = crate::booktext::esc(wanted),
        );
        if let Err(e) = crate::register::send_mail(state, &old, "Your Legejo email address is about to change", &notice).await {
            tracing::warn!("email change notice to the old address failed: {e:#}");
        }
    }
    Ok(Some("confirm"))
}

#[derive(Deserialize)]
pub struct ConfirmEmail {
    token: String,
}

/// POST /api/account/email/confirm: the link from the mail to the new
/// address, opened. It may be opened anywhere, logged in or not: the token
/// is the proof.
pub async fn confirm_email(State(state): State<AppState>, Json(req): Json<ConfirmEmail>) -> Result<Json<serde_json::Value>, Response> {
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT id, username, pending_email FROM users
         WHERE email_token = $1 AND email_token_expires > $2 AND pending_email IS NOT NULL",
    )
    .bind(req.token.trim())
    .bind(crate::db::now_ts())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let Some((id, username, email)) = row else {
        return Err((StatusCode::GONE, Json(serde_json::json!({ "error": "the link is invalid or has expired" }))).into_response());
    };
    let set = sqlx::query("UPDATE users SET email = $1, pending_email = NULL, email_token = NULL, email_token_expires = NULL WHERE id = $2")
        .bind(&email)
        .bind(id)
        .execute(&state.db)
        .await;
    match set {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "that email is in use" }))).into_response());
        }
        Err(e) => return Err(internal(e.into())),
    }
    crate::audit::log(&state, Some((id, &username)), "account.email_confirmed", serde_json::json!({})).await;
    Ok(Json(serde_json::json!({ "username": username, "email": email })))
}

fn avatar_path(state: &AppState, user_id: i64) -> std::path::PathBuf {
    state.data_dir.join("avatars").join(user_id.to_string())
}

pub async fn upload_avatar(
    State(state): State<AppState>,
    user: AuthUser,
    mut multipart: Multipart,
) -> Result<StatusCode, Response> {
    let (bytes, mime) = crate::images::avatar(&state, &mut multipart).await?;
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
    Ok((crate::images::headers(&mime), [(header::CACHE_CONTROL, "private, no-cache")], data).into_response())
}

pub(crate) const LOCALES: [&str; 7] = ["en", "sv", "fi", "eo", "fr", "de", "es"];

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
    if !crate::db::verify_password(&hash, &req.password).await.map_err(internal)? {
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

    let books = remove_user(&state, user.0.id, &user.0.username).await?;
    crate::audit::log(
        &state,
        Some((user.0.id, user.0.username.as_str())),
        "user.deleted",
        serde_json::json!({ "username": user.0.username, "books": books }),
    )
    .await;
    let mut removal = axum_extra::extract::cookie::Cookie::from(auth::SESSION_COOKIE);
    removal.set_path("/");
    Ok(jar.remove(removal))
}

/// An account and everything in it, gone: its federated shelves retracted
/// so that followers drop them, its books, audiobooks and pictures off the
/// disk, and the rest by the database's cascades. The number of books it
/// had. Asked for by the user themselves or by an admin.
pub(crate) async fn remove_user(state: &AppState, user_id: i64, username: &str) -> Result<usize, Response> {
    let state = state.clone();
    // Retract federated shelves so followers drop them.
    if let Some(c) = crate::fed::active(&state).await {
        let federated: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, ap_slug FROM shelves WHERE owner_id = $1 AND visibility = 'federated' AND ap_slug IS NOT NULL",
        )
        .bind(user_id)
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
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for (id, uuid) in &books {
        crate::books::remove_book(&state, user_id, *id, uuid).await?;
    }
    let audiobooks: Vec<String> = sqlx::query_scalar("SELECT uuid FROM audiobooks WHERE owner_id = $1")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for uuid in &audiobooks {
        crate::audiobooks::remove_files(&state, uuid).await;
    }
    let shelves: Vec<i64> = sqlx::query_scalar("SELECT id FROM shelves WHERE owner_id = $1")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    for id in shelves {
        let _ = tokio::fs::remove_file(state.data_dir.join("shelf_covers").join(id.to_string())).await;
    }
    let _ = tokio::fs::remove_file(avatar_path(&state, user_id)).await;
    let _ = tokio::fs::remove_dir_all(state.data_dir.join("exports").join(user_id.to_string())).await;

    // Everything else (shelves, sessions, progress, follows, exports) cascades.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    state.fed.wake.notify_one();
    tracing::info!("account {username} deleted ({} books)", books.len());
    Ok(books.len())
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    #[tokio::test]
    async fn renaming_follows_the_username_rule() {
        let (app, db, _) = test_app().await;
        let (ada, cookie) = add_user(&db, "ada").await;
        let hash = crate::db::hash_password("adas lösenord").await.unwrap();
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?").bind(&hash).bind(ada).execute(&db).await.unwrap();
        let rename = |name: String| {
            let app = app.clone();
            let cookie = cookie.clone();
            async move {
                send(&app, Method::PUT, "/api/account", Some(&cookie), Some(json!({ "current_password": "adas lösenord", "username": name }))).await
            }
        };
        for bad in ["", "   ", &"a".repeat(61), "ada\u{1f}"] {
            let (status, _) = rename(bad.to_string()).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad:?}");
        }
        let (status, v) = rename("  Ada Lovelace ".into()).await;
        assert_eq!((status, v["username"].clone()), (StatusCode::OK, json!("Ada Lovelace")));
        // The wrong password changes nothing.
        let (status, _) = send(&app, Method::PUT, "/api/account", Some(&cookie), Some(json!({ "current_password": "fel", "username": "x" }))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

/// GET /api/account/kobo/devices: the Kobo devices that have synced with
/// this account, and how the last sync went, from the system log.
pub async fn kobo_devices(State(state): State<AppState>, user: AuthUser) -> Result<Json<serde_json::Value>, Response> {
    let devices: Vec<(Option<String>, String)> =
        sqlx::query_as("SELECT model, last_seen_at FROM kobo_devices WHERE user_id = $1 ORDER BY last_seen_at DESC")
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    let last: Option<(String, String, String)> = sqlx::query_as(
        "SELECT at, action, details FROM activity_log WHERE actor_id = $1 AND action IN ('kobo.synced', 'kobo.sync_failed')
         ORDER BY id DESC LIMIT 1",
    )
    .bind(user.0.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let month: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE actor_id = $1 AND action = 'kobo.synced' AND at > $2")
        .bind(user.0.id)
        .bind(crate::db::ts_in_hours(-24 * 30))
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let last_sync = last.map(|(at, action, details)| {
        let d: serde_json::Value = serde_json::from_str(&details).unwrap_or_default();
        let n = |key: &str| d.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        serde_json::json!({
            "at": at,
            "ok": action == "kobo.synced",
            "new": n("new"), "changed": n("changed"), "collections": n("collections"), "states": n("states"),
            "status": d.get("status").and_then(|v| v.as_i64()),
        })
    });
    Ok(Json(serde_json::json!({
        "devices": devices.into_iter().map(|(model, last_seen_at)| serde_json::json!({ "model": model, "last_seen_at": last_seen_at })).collect::<Vec<_>>(),
        "last_sync": last_sync,
        "syncs_last_30_days": month,
    })))
}
