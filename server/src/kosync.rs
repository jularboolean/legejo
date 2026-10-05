//! KOReader progress sync ("kosync"), the koreader-sync-server protocol,
//! mounted under /api/kosync. KOReader reports its position here; the
//! percentage feeds the book's reading progress (latest of web, Kobo and
//! KOReader wins, see progress.rs) and the position is handed back to the
//! user's other KOReader devices.
//!
//! Endpoints used by KOReader's kosync plugin:
//!   GET  /users/auth                      headers x-auth-user, x-auth-key → {"authorized":"OK"}
//!   POST /users/create                    (refused: the key comes from the Account page)
//!   PUT  /syncs/progress                  {document, progress, percentage, device, device_id}
//!   GET  /syncs/progress/{document}       → the stored position, or {}
//!   GET  /healthcheck                     → {"state":"OK"}
//! x-auth-key is md5(password); the "password" is the sync key from the
//! Account page (stored like the Kobo token), never the login password.
//! A document is KOReader's partial MD5 of the file (`partial_md5`), which we
//! compute for every book (books.koreader_md5).

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use md5::{Digest, Md5};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthcheck", get(|| async { Json(json!({ "state": "OK" })) }))
        .route("/users/auth", get(auth))
        .route("/users/create", post(create))
        .route("/syncs/progress", put(put_progress))
        .route("/syncs/progress/{document}", get(get_progress))
        .fallback(|| async { StatusCode::NOT_FOUND })
}

/// KOReader's document id: MD5 over 1 KiB samples at offsets 0 and
/// 1024·4^i (i = 0…10), stopping at the end of the file. Mirrors
/// `util.partialMD5` in KOReader (frontend/util.lua).
pub fn partial_md5(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; 1024];
    for i in -1i32..=10 {
        let offset: u64 = if i < 0 { 0 } else { 1024u64 << (2 * i) };
        if offset >= len {
            break;
        }
        f.seek(SeekFrom::Start(offset))?;
        let mut n = 0;
        while n < buf.len() {
            let r = f.read(&mut buf[n..])?;
            if r == 0 {
                break;
            }
            n += r;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn md5_hex(s: &str) -> String {
    Md5::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn kerr(status: StatusCode, code: i64, message: &str) -> Response {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

fn unauthorized() -> Response {
    kerr(StatusCode::UNAUTHORIZED, 2001, "Unauthorized")
}

/// The user behind x-auth-user / x-auth-key, if the key matches.
async fn user_from_headers(state: &AppState, headers: &HeaderMap) -> Result<i64, Response> {
    let user = headers.get("x-auth-user").and_then(|v| v.to_str().ok()).ok_or_else(unauthorized)?;
    let key = headers.get("x-auth-key").and_then(|v| v.to_str().ok()).ok_or_else(unauthorized)?;
    let row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, kosync_key FROM users WHERE LOWER(username) = LOWER($1)")
            .bind(user.trim())
            .fetch_optional(&state.db)
            .await
            .map_err(|_| unauthorized())?;
    let (id, stored) = row.ok_or_else(unauthorized)?;
    let stored = stored.ok_or_else(unauthorized)?;
    let expected = md5_hex(&stored);
    // Constant-time comparison.
    let given = key.trim().to_ascii_lowercase();
    let same = expected.len() == given.len()
        && expected.bytes().zip(given.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    if same { Ok(id) } else { Err(unauthorized()) }
}

async fn auth(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, Response> {
    user_from_headers(&state, &headers).await?;
    Ok(Json(json!({ "authorized": "OK" })))
}

/// KOReader's "Register" button. Accounts and keys come from Legejo itself.
async fn create() -> Response {
    kerr(
        StatusCode::PAYMENT_REQUIRED,
        2002,
        "Registration is not available here. Use Login with your Legejo username and the sync key from your Legejo account page.",
    )
}

#[derive(Deserialize)]
struct PutProgress {
    document: String,
    progress: String,
    percentage: f64,
    device: Option<String>,
    device_id: Option<String>,
}

fn valid_document(d: &str) -> bool {
    !d.is_empty() && d.len() <= 64 && d.bytes().all(|b| b.is_ascii_alphanumeric())
}

async fn put_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PutProgress>,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_headers(&state, &headers).await?;
    if !valid_document(&req.document) || req.progress.len() > 4000 || !(0.0..=1.0).contains(&req.percentage) {
        return Err(kerr(StatusCode::FORBIDDEN, 2003, "Invalid request"));
    }
    let document = req.document.to_ascii_lowercase();
    let book_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM books WHERE owner_id = $1 AND koreader_md5 = $2 ORDER BY id LIMIT 1")
            .bind(user_id)
            .bind(&document)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e))?;
    let now = now_ts();
    if let Some(book) = book_id {
        // Detach the book from documents of older files (e.g. before a cover change).
        sqlx::query("UPDATE kosync_progress SET book_id = NULL WHERE user_id = $1 AND book_id = $2 AND document <> $3")
            .bind(user_id)
            .bind(book)
            .bind(&document)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e))?;
    }
    let trunc = |s: Option<String>| s.map(|v| v.chars().take(100).collect::<String>());
    // Two statements rather than a nullable book_id bind (see books::float_param).
    let (device, device_id) = (trunc(req.device), trunc(req.device_id));
    let percentage = req.percentage.to_string();
    let result = match book_id {
        Some(book) => {
            sqlx::query(
                "INSERT INTO kosync_progress (user_id, document, book_id, progress, percentage, device, device_id, updated_at)
                 VALUES ($1, $2, $3, $4, CAST($5 AS DOUBLE PRECISION), $6, $7, $8)
                 ON CONFLICT (user_id, document) DO UPDATE SET book_id = excluded.book_id, progress = excluded.progress,
                     percentage = excluded.percentage, device = excluded.device, device_id = excluded.device_id,
                     updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(&document)
            .bind(book)
            .bind(&req.progress)
            .bind(&percentage)
            .bind(&device)
            .bind(&device_id)
            .bind(&now)
            .execute(&state.db)
            .await
        }
        None => {
            sqlx::query(
                "INSERT INTO kosync_progress (user_id, document, progress, percentage, device, device_id, updated_at)
                 VALUES ($1, $2, $3, CAST($4 AS DOUBLE PRECISION), $5, $6, $7)
                 ON CONFLICT (user_id, document) DO UPDATE SET progress = excluded.progress,
                     percentage = excluded.percentage, device = excluded.device, device_id = excluded.device_id,
                     updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(&document)
            .bind(&req.progress)
            .bind(&percentage)
            .bind(&device)
            .bind(&device_id)
            .bind(&now)
            .execute(&state.db)
            .await
        }
    };
    result.map_err(|e| internal(e))?;
    Ok(Json(json!({ "document": document, "timestamp": unix_now() })))
}

fn unix_now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn unix_of(ts: &str) -> i64 {
    time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339)
        .map(|t| t.unix_timestamp())
        .unwrap_or(0)
}

async fn get_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document): Path<String>,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_headers(&state, &headers).await?;
    if !valid_document(&document) {
        return Err(kerr(StatusCode::FORBIDDEN, 2003, "Invalid request"));
    }
    let row: Option<(String, f64, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT progress, percentage, device, device_id, updated_at FROM kosync_progress WHERE user_id = $1 AND document = $2",
    )
    .bind(user_id)
    .bind(document.to_ascii_lowercase())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e))?;
    // Only KOReader's own positions are returned: web and Kobo positions have
    // no xpointer KOReader could jump to.
    Ok(Json(match row {
        Some((progress, percentage, device, device_id, at)) => json!({
            "document": document.to_ascii_lowercase(),
            "progress": progress,
            "percentage": percentage,
            "device": device,
            "device_id": device_id,
            "timestamp": unix_of(&at),
        }),
        None => json!({}),
    }))
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("kosync: {e}");
    kerr(StatusCode::INTERNAL_SERVER_ERROR, 2000, "Unknown server error")
}

// ------------------------------------------------- Account page: the key

/// POST /api/account/kosync-key: issue a new sync key, invalidating the old one.
pub async fn create_key(State(state): State<AppState>, user: AuthUser) -> Result<Json<Value>, Response> {
    // Easy to type on an e-reader keyboard: lowercase letters and digits.
    let key: String = {
        use rand::Rng;
        const CHARS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
        let mut rng = rand::thread_rng();
        (0..20).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
    };
    sqlx::query("UPDATE users SET kosync_key = $1 WHERE id = $2")
        .bind(&key)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e))?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.kosync_key_created", json!({})).await;
    Ok(Json(json!({ "key": key })))
}

pub async fn delete_key(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, Response> {
    sqlx::query("UPDATE users SET kosync_key = NULL WHERE id = $1")
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e))?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.kosync_key_removed", json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/account/kosync-key: the current key, or null.
pub async fn get_key(State(state): State<AppState>, user: AuthUser) -> Result<Json<Value>, Response> {
    let key: Option<String> = sqlx::query_scalar("SELECT kosync_key FROM users WHERE id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e))?;
    Ok(Json(json!({ "key": key })))
}

/// Compute koreader_md5 for books that lack it (older books, changed files).
pub async fn backfill(state: AppState) {
    let rows: Vec<(i64, String)> =
        match sqlx::query_as("SELECT id, uuid FROM books WHERE koreader_md5 IS NULL").fetch_all(&state.db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("koreader md5 backfill skipped: {e}");
                return;
            }
        };
    let mut done = 0;
    for (id, uuid) in rows {
        if set_md5(&state, id, &uuid).await {
            done += 1;
        }
    }
    if done > 0 {
        tracing::info!("computed KOReader id for {done} books");
    }
}

/// (Re)compute one book's KOReader id from its file.
pub async fn set_md5(state: &AppState, id: i64, uuid: &str) -> bool {
    let path = state.data_dir.join("books").join(format!("{uuid}.epub"));
    let Ok(Ok(md5)) = tokio::task::spawn_blocking(move || partial_md5(&path)).await else { return false };
    sqlx::query("UPDATE books SET koreader_md5 = $1 WHERE id = $2")
        .bind(md5)
        .bind(id)
        .execute(&state.db)
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_md5_samples_like_koreader() {
        let dir = std::env::temp_dir().join(format!("kosync-{}", crate::books::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        // Small file: only the sample at 0 (and the one at 1024 if longer).
        let small = dir.join("small");
        std::fs::write(&small, b"abc").unwrap();
        assert_eq!(partial_md5(&small).unwrap(), md5_hex("abc"));
        // 3000 bytes: samples at 0 (1024 bytes) and 1024 (1024 bytes); 4096 is past the end.
        let bytes: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let mid = dir.join("mid");
        std::fs::write(&mid, &bytes).unwrap();
        let mut h = Md5::new();
        h.update(&bytes[0..1024]);
        h.update(&bytes[1024..2048]);
        let expected: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(partial_md5(&mid).unwrap(), expected);
    }
}
