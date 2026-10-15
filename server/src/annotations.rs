//! Highlights and notes in a book: made in the web reader, or (later) brought
//! in from a Kobo. Each belongs to the book's owner; the passage is stored
//! with it, so it can be listed and exported whatever placed it.

use crate::auth::AuthUser;
use crate::db::{now_ts, DbFlag};
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
    /// Where a device put it, as the device said (JSON); for placing it.
    pub location: Option<String>,
    /// Chapter, share of the book and printed page, as the reader found them.
    #[serde(serialize_with = "as_json")]
    pub place: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str = "id, book_id, source, cfi, text, note, color, location, place, created_at, updated_at";

/// A JSON column given as the value it holds, not as a string of it.
fn as_json<S: serde::Serializer>(raw: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
    let value: Option<serde_json::Value> = raw.as_deref().and_then(|r| serde_json::from_str(r).ok());
    serde::Serialize::serialize(&value, s)
}

/// What the reader says of a highlight's place, kept to what is shown:
/// a chapter's name, a share of the book (0-1) and a printed page.
fn clean_place(place: Option<serde_json::Value>) -> Option<String> {
    let place = place?;
    let mut out = serde_json::Map::new();
    if let Some(chapter) = place.get("chapter").and_then(|v| v.as_str()).map(str::trim).filter(|c| !c.is_empty()) {
        out.insert("chapter".into(), json!(chapter.chars().take(200).collect::<String>()));
    }
    if let Some(percent) = place.get("percent").and_then(|v| v.as_f64()).filter(|p| (0.0..=1.0).contains(p)) {
        out.insert("percent".into(), json!((percent * 1000.0).round() / 1000.0));
    }
    let page = match place.get("page") {
        Some(serde_json::Value::String(p)) => Some(p.trim().to_string()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    if let Some(page) = page.filter(|p| !p.is_empty() && p.chars().count() <= 20) {
        out.insert("page".into(), json!(page));
    }
    (!out.is_empty()).then(|| serde_json::Value::Object(out).to_string())
}

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
    #[serde(default)]
    place: Option<serde_json::Value>,
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
        "INSERT INTO annotations (book_id, user_id, source, cfi, text, note, place, created_at, updated_at)
         VALUES ($1, $2, 'web', $3, $4, $5, $7, $6, $6) RETURNING id",
    )
    .bind(book_id)
    .bind(user.0.id)
    .bind(cfi)
    .bind(&text)
    .bind(&note)
    .bind(&now)
    .bind(clean_place(req.place))
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
    /// Where the reader found a passage that came without a place (from a
    /// device). Absent: unchanged.
    cfi: Option<String>,
    /// Chapter, share and page, as the reader found them. Absent: unchanged.
    #[serde(default)]
    place: Option<serde_json::Value>,
}

/// PUT /api/annotations/{id}: the note, or the place.
pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<Update>,
) -> Result<Json<Annotation>, Response> {
    let current = fetch(&state, user.0.id, id).await?;
    // The place is the reader's bookkeeping, not a change to the highlight:
    // written without touching updated_at, which devices go by.
    if let Some(place) = clean_place(req.place) {
        sqlx::query("UPDATE annotations SET place = $1 WHERE id = $2")
            .bind(place)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(internal)?;
        if req.note.is_none() && req.cfi.is_none() {
            return Ok(Json(fetch(&state, user.0.id, id).await?));
        }
    }
    let note = match req.note {
        Some(note) => clean_note(note)?,
        None => current.note,
    };
    let cfi = match req.cfi.map(|c| c.trim().to_string()) {
        Some(c) if c.is_empty() || c.chars().count() > MAX_CFI_CHARS => return Err(unprocessable("cfi missing or too long")),
        Some(c) => Some(c),
        None => current.cfi,
    };
    sqlx::query("UPDATE annotations SET note = $1, cfi = $4, updated_at = $2 WHERE id = $3")
        .bind(&note)
        .bind(now_ts())
        .bind(id)
        .bind(&cfi)
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

/// A highlight with the book it is in, for the list of them all.
#[derive(Serialize, sqlx::FromRow)]
pub struct InBook {
    pub id: i64,
    pub book_id: i64,
    pub source: String,
    pub cfi: Option<String>,
    pub text: String,
    pub note: Option<String>,
    pub color: Option<String>,
    /// As the device said it, for a highlight from a Kobo (JSON).
    #[serde(serialize_with = "as_json")]
    pub location: Option<String>,
    #[serde(serialize_with = "as_json")]
    pub place: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub book_title: String,
    pub book_author: Option<String>,
    pub book_has_cover: DbFlag,
    /// What a cover's address carries so that a changed cover is fetched anew.
    pub book_changed_at: String,
}

/// GET /api/annotations: every highlight and note the user has made, in
/// every book of their own, with the book. Ordered by book and then as made;
/// the page orders them as they stand in the book.
pub async fn all(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<InBook>>, Response> {
    let rows: Vec<InBook> = sqlx::query_as(
        "SELECT a.id, a.book_id, a.source, a.cfi, a.text, a.note, a.color, a.location, a.place, a.created_at, a.updated_at,
                b.title AS book_title, b.author AS book_author,
                CAST(CASE WHEN b.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS book_has_cover,
                COALESCE(b.updated_at, b.created_at) AS book_changed_at
         FROM annotations a JOIN books b ON b.id = a.book_id
         WHERE a.user_id = $1 AND b.owner_id = $1
         ORDER BY a.book_id, a.id",
    )
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    Ok(Json(rows))
}

/// GET /api/annotations/export: every book's highlights in one Markdown file.
pub async fn export_all(State(state): State<AppState>, user: AuthUser) -> Result<Response, Response> {
    let books: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT b.id, b.title, b.author FROM books b JOIN annotations a ON a.book_id = b.id
         WHERE a.user_id = $1 AND b.owner_id = $1 ORDER BY b.title",
    )
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let mut parts = Vec::with_capacity(books.len());
    for (book_id, title, author) in books {
        let rows: Vec<Annotation> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM annotations WHERE book_id = $1 AND user_id = $2 ORDER BY id"))
                .bind(book_id)
                .bind(user.0.id)
                .fetch_all(&state.db)
                .await
                .map_err(internal)?;
        parts.push(markdown(&title, author.as_deref(), &rows));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"notes.md\"".to_string()),
        ],
        parts.join("\n"),
    )
        .into_response())
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
