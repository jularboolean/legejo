//! Highlights and notes in a book: made in the web reader, or (later) brought
//! in from a Kobo. Each belongs to the book's owner; the passage is stored
//! with it, so it can be listed and exported whatever placed it.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The reader's CFIs stay well under this; the progress API has the same cap.
const MAX_CFI_CHARS: usize = 2000;
/// A passage, as text. Longer than a long paragraph is not a highlight.
const MAX_TEXT_CHARS: usize = 4000;
const MAX_NOTE_CHARS: usize = 10_000;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
}

fn unprocessable(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": msg }))).into_response()
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Annotation {
    pub id: i64,
    pub book_id: i64,
    /// "web" or "kobo".
    pub source: String,
    /// Where in the book, as a range CFI; absent for a passage the reader
    /// has not placed yet.
    pub cfi: Option<String>,
    pub text: String,
    pub note: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str = "id, book_id, source, cfi, text, note, color, created_at, updated_at";

fn clean_note(note: Option<String>) -> Result<Option<String>, Response> {
    let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS) {
        return Err(unprocessable("note too long"));
    }
    Ok(note)
}

async fn owns_book(state: &AppState, user_id: i64, book_id: i64) -> Result<(), Response> {
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM books WHERE id = $1 AND owner_id = $2")
        .bind(book_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    found.map(|_| ()).ok_or_else(not_found)
}

async fn fetch(state: &AppState, user_id: i64, id: i64) -> Result<Annotation, Response> {
    let row: Option<Annotation> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM annotations WHERE id = $1 AND user_id = $2"))
        .bind(id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    row.ok_or_else(not_found)
}

/// How many the owner has in a book; for the book page.
pub async fn count(state: &AppState, user_id: i64, book_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM annotations WHERE book_id = $1 AND user_id = $2")
        .bind(book_id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
}

/// GET /api/books/{id}/annotations: in the order they were made.
pub async fn list(State(state): State<AppState>, user: AuthUser, Path(book_id): Path<i64>) -> Result<Json<Vec<Annotation>>, Response> {
    owns_book(&state, user.0.id, book_id).await?;
    let rows: Vec<Annotation> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM annotations WHERE book_id = $1 AND user_id = $2 ORDER BY id"))
            .bind(book_id)
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
            .map_err(internal)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct Create {
    cfi: String,
    text: String,
    note: Option<String>,
}

/// POST /api/books/{id}/annotations: a highlight, with a note when given.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(book_id): Path<i64>,
    Json(req): Json<Create>,
) -> Result<(StatusCode, Json<Annotation>), Response> {
    owns_book(&state, user.0.id, book_id).await?;
    let cfi = req.cfi.trim();
    if cfi.is_empty() || cfi.chars().count() > MAX_CFI_CHARS {
        return Err(unprocessable("cfi missing or too long"));
    }
    let text: String = req.text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS {
        return Err(unprocessable("text missing or too long"));
    }
    let note = clean_note(req.note)?;
    let now = now_ts();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO annotations (book_id, user_id, source, cfi, text, note, created_at, updated_at)
         VALUES ($1, $2, 'web', $3, $4, $5, $6, $6) RETURNING id",
    )
    .bind(book_id)
    .bind(user.0.id)
    .bind(cfi)
    .bind(&text)
    .bind(&note)
    .bind(&now)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    Ok((StatusCode::CREATED, Json(fetch(&state, user.0.id, id).await?)))
}

#[derive(Deserialize)]
pub struct Update {
    /// Absent: unchanged. Empty or null: the note goes.
    #[serde(default, deserialize_with = "crate::books::deserialize_some")]
    note: Option<Option<String>>,
}

/// PUT /api/annotations/{id}: the note.
pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<Update>,
) -> Result<Json<Annotation>, Response> {
    let current = fetch(&state, user.0.id, id).await?;
    let note = match req.note {
        Some(note) => clean_note(note)?,
        None => current.note,
    };
    sqlx::query("UPDATE annotations SET note = $1, updated_at = $2 WHERE id = $3")
        .bind(&note)
        .bind(now_ts())
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    Ok(Json(fetch(&state, user.0.id, id).await?))
}

/// DELETE /api/annotations/{id}
pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    let done = sqlx::query("DELETE FROM annotations WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if done.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The annotations of a book as Markdown: a quote per passage, the note
/// below it.
pub fn markdown(title: &str, author: Option<&str>, annotations: &[Annotation]) -> String {
    let mut out = format!("# {title}\n");
    if let Some(author) = author.filter(|a| !a.trim().is_empty()) {
        out.push_str(&format!("\n{author}\n"));
    }
    for a in annotations {
        out.push('\n');
        for line in a.text.lines() {
            out.push_str(&format!("> {line}\n"));
        }
        if let Some(note) = a.note.as_deref().filter(|n| !n.is_empty()) {
            out.push('\n');
            out.push_str(note.trim());
            out.push('\n');
        }
    }
    out
}

/// GET /api/books/{id}/annotations/export: a Markdown file to save.
pub async fn export(State(state): State<AppState>, user: AuthUser, Path(book_id): Path<i64>) -> Result<Response, Response> {
    let book: Option<(String, Option<String>)> = sqlx::query_as("SELECT title, author FROM books WHERE id = $1 AND owner_id = $2")
        .bind(book_id)
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    let (title, author) = book.ok_or_else(not_found)?;
    let rows: Vec<Annotation> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM annotations WHERE book_id = $1 AND user_id = $2 ORDER BY id"))
            .bind(book_id)
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
            .map_err(internal)?;
    let body = markdown(&title, author.as_deref(), &rows);
    // The title as a file name: letters, digits and a few marks; the rest a space.
    let stem: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | ',') { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let filename = format!("{}.md", if stem.is_empty() { "annotations".to_string() } else { stem });
    let encoded: String = url::form_urlencoded::byte_serialize(filename.as_bytes()).collect::<String>().replace('+', "%20");
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{encoded}")),
        ],
        body,
    )
        .into_response())
}
