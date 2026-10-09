use crate::auth::AuthUser;
use crate::db::{now_ts, Backend, DbFlag};
use crate::epubfix::{self, Health};
use crate::formats::Format;
use crate::license::{self, License, LicenseFacts, NotFederable};
use crate::progress;
use crate::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use epub::doc::EpubDoc;
use rand::RngCore;
use serde::Serialize;
use std::io::Cursor;

#[derive(Serialize, sqlx::FromRow)]
pub struct Book {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    /// This edition's date: "YYYY" or "YYYY-MM-DD".
    pub published: Option<String>,
    /// The year the work first appeared.
    pub first_published: Option<i64>,
    pub category: Option<String>,
    pub identifier: Option<String>,
    pub isbn: Option<String>,
    pub libris_id: Option<String>,
    /// For imported copies: the uuid of the original book (chains flatten
    /// to the first origin).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uuid: Option<String>,
    /// Series name and position, the calibre model (float allows "1.5").
    pub series: Option<String>,
    pub series_index: Option<f64>,
    /// The owner's rating, 1–5.
    pub rating: Option<i64>,
    /// Licence facts (license.rs): the licence name, where the free edition
    /// comes from, and for public domain the last author's year of death.
    pub license: Option<String>,
    pub license_source_url: Option<String>,
    pub author_death_year: Option<i64>,
    /// The owner confirmed that the cover, too, is free.
    pub cover_is_free: DbFlag,
    /// For books fetched from a federated shelf: the remote object's IRI.
    pub fed_source: Option<String>,
    /// On the owner's want-to-read list.
    pub want_to_read: DbFlag,
    pub wanted_at: Option<String>,
    /// epub | pdf | cbz (formats.rs). Only EPUB is read, tended and converted.
    #[sqlx(default)]
    pub format: String,
    pub file_size: i64,
    /// How many things about the file need attention (see epubfix::Issue).
    #[sqlx(default)]
    pub health_issues: i64,
    pub has_cover: DbFlag,
    pub created_at: String,
    pub updated_at: Option<String>,
    /// Reading progress 0–1 for the requesting user (web or Kobo, latest wins).
    /// Only filled in by the web API's queries; omitted when there is none.
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f64>,
    /// When the requesting user last read it (web or Kobo); library listing only.
    #[sqlx(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_read_at: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ShelfRef {
    pub id: i64,
    pub name: String,
}

/// A book in the library listing, with the shelves it sits on.
#[derive(Serialize)]
pub struct BookListItem {
    #[serde(flatten)]
    pub book: Book,
    pub shelf_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct BookDetail {
    #[serde(flatten)]
    pub book: Book,
    pub shelves: Vec<ShelfRef>,
    pub tags: Vec<String>,
    /// Whether the book could go on a federated shelf, and if not, why.
    pub federable: FederableStatus,
    /// The owner removed it from their Kobo; it is left out of the sync.
    pub kobo_removed: bool,
    /// What the health check says about the file; absent until it has run.
    pub health: Option<Health>,
    /// The owner's books just before and after this one in its series, by
    /// series index; absent when there is none, or the book has no index.
    pub previous_in_series: Option<SeriesNeighbour>,
    pub next_in_series: Option<SeriesNeighbour>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SeriesNeighbour {
    pub id: i64,
    pub title: String,
    pub series_index: Option<f64>,
}

/// The owner's nearest book on one side of `book` in its series.
async fn series_neighbour(state: &AppState, owner_id: i64, book: &Book, after: bool) -> Result<Option<SeriesNeighbour>, Response> {
    let (Some(series), Some(index)) = (&book.series, book.series_index) else { return Ok(None) };
    let (cmp, order) = if after { (">", "ASC") } else { ("<", "DESC") };
    sqlx::query_as(&format!(
        "SELECT id, title, series_index FROM books
         WHERE owner_id = $1 AND id <> $2 AND series IS NOT NULL AND LOWER(series) = LOWER($3)
           AND series_index IS NOT NULL AND series_index {cmp} CAST($4 AS DOUBLE PRECISION)
         ORDER BY series_index {order}, LOWER(title) LIMIT 1"
    ))
    .bind(owner_id)
    .bind(book.id)
    .bind(series)
    .bind(float_param(Some(index)))
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))
}

/// `{"ok": true}` or `{"ok": false, "code": "missing_source", …}`.
#[derive(Serialize)]
pub struct FederableStatus {
    pub ok: bool,
    #[serde(flatten)]
    pub reason: Option<NotFederable>,
}

impl Book {
    pub fn license_facts(&self) -> LicenseFacts {
        LicenseFacts {
            license: self.license.clone(),
            source_url: self.license_source_url.clone(),
            author_death_year: self.author_death_year,
        }
    }

    pub fn federable_status(&self) -> FederableStatus {
        match license::federable(&self.license_facts(), license::current_year()) {
            Ok(()) => FederableStatus { ok: true, reason: None },
            Err(reason) => FederableStatus { ok: false, reason: Some(reason) },
        }
    }
}

// has_cover is CAST(... AS BIGINT): the portable spelling both engines
// return as an integer the Any driver can decode (see db::DbFlag).
pub(crate) const BOOK_COLUMNS: &str =
    "id, uuid, title, author, language, description, publisher, published, first_published, \
     category, identifier, isbn, libris_id, source_uuid, series, series_index, rating, \
     license, license_source_url, author_death_year, cover_is_free, fed_source, want_to_read, wanted_at, format, file_size, health_issues, \
     CAST(CASE WHEN cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover, \
     created_at, updated_at";

/// Same columns qualified with the `b` table alias, for queries that join other tables.
pub(crate) const BOOK_COLUMNS_B: &str =
    "b.id, b.uuid, b.title, b.author, b.language, b.description, b.publisher, b.published, b.first_published, \
     b.category, b.identifier, b.isbn, b.libris_id, b.source_uuid, b.series, b.series_index, b.rating, \
     b.license, b.license_source_url, b.author_death_year, b.cover_is_free, b.fed_source, b.want_to_read, b.wanted_at, b.format, b.file_size, b.health_issues, \
     CAST(CASE WHEN b.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover, \
     b.created_at, b.updated_at";

/// Where a book's file is kept.
pub(crate) fn book_path(state: &AppState, uuid: &str, format: Format) -> std::path::PathBuf {
    state.data_dir.join("books").join(format!("{uuid}.{}", format.as_str()))
}

/// A book's file and its format, by the book's uuid. A uuid no book has
/// counts as EPUB, so that the caller's own "not found" stays in charge.
pub(crate) async fn book_file(state: &AppState, uuid: &str) -> (std::path::PathBuf, Format) {
    let name: Option<String> =
        sqlx::query_scalar("SELECT format FROM books WHERE uuid = $1").bind(uuid).fetch_optional(&state.db).await.ok().flatten();
    let format = Format::parse(name.as_deref().unwrap_or("epub"));
    (book_path(state, uuid, format), format)
}

/// A title as a file name, for a download.
pub(crate) fn download_name(title: &str, format: Format) -> String {
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    format!("{safe}.{}", format.as_str())
}

async fn book_shelves(state: &AppState, book_id: i64) -> Result<Vec<ShelfRef>, Response> {
    sqlx::query_as(
        "SELECT s.id, s.name FROM shelves s
         JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE sb.book_id = $1
         ORDER BY LOWER(s.name)",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))
}

pub(crate) async fn book_tags(state: &AppState, book_id: i64) -> Result<Vec<String>, Response> {
    sqlx::query_scalar("SELECT tag FROM book_tags WHERE book_id = $1 ORDER BY LOWER(tag)")
        .bind(book_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response()
}

#[derive(serde::Deserialize)]
pub struct ListParams {
    q: Option<String>,
}

impl ListParams {
    pub(crate) fn new(q: Option<String>) -> Self {
        ListParams { q }
    }
}

/// Full-text search, the one place where the two backends truly differ.
/// Returns (extra JOIN, WHERE condition on $2, ORDER BY expression).
pub(crate) struct SearchParts {
    pub join: &'static str,
    pub condition: &'static str,
    pub order: &'static str,
}

/// Must match the expression in the idx_books_fts GIN index exactly.
macro_rules! pg_tsvector {
    () => {
        "to_tsvector('simple', coalesce(b.title, '') || ' ' || coalesce(b.author, '') || ' ' || \
         coalesce(b.description, '') || ' ' || coalesce(b.category, ''))"
    };
}

pub(crate) fn search_parts(backend: Backend) -> SearchParts {
    match backend {
        Backend::Sqlite => SearchParts {
            join: "JOIN books_fts f ON f.rowid = b.id",
            condition: "books_fts MATCH $2",
            order: "f.rank",
        },
        Backend::Postgres => SearchParts {
            join: "",
            condition: concat!(pg_tsvector!(), " @@ to_tsquery('simple', $2)"),
            order: concat!("ts_rank(", pg_tsvector!(), ", to_tsquery('simple', $2)) DESC"),
        },
    }
}

/// Turn raw user input into the backend's prefix-search expression; syntax
/// characters are stripped so they can't break the query.
pub(crate) fn search_expr(backend: Backend, input: &str) -> Option<String> {
    let tokens: Vec<String> = match backend {
        Backend::Sqlite => input
            .split_whitespace()
            .map(|t| t.replace('"', ""))
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{t}\"*"))
            .collect(),
        Backend::Postgres => input
            .split_whitespace()
            .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
            .filter(|t| !t.is_empty())
            .map(|t| format!("{t}:*"))
            .collect(),
    };
    let separator = match backend {
        Backend::Sqlite => " ",
        Backend::Postgres => " & ",
    };
    (!tokens.is_empty()).then(|| tokens.join(separator))
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> Result<Json<Vec<BookListItem>>, Response> {
    let query = params.q.as_deref().and_then(|q| search_expr(state.backend, q));
    let percent = progress::progress_percent(state.backend);
    // When it was last read: the timestamp of whichever source wins the progress.
    let last_read = progress::last_read_at(state.backend);
    let joins = progress::PROGRESS_JOINS;

    let books: Vec<Book> = match query {
        Some(match_expr) => {
            let s = search_parts(state.backend);
            sqlx::query_as(&format!(
                "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent, {last_read} AS last_read_at FROM books b
                 {search_join}
                 {joins}
                 WHERE b.owner_id = $1 AND {condition}
                 ORDER BY {order}",
                search_join = s.join,
                condition = s.condition,
                order = s.order,
            ))
            .bind(user.0.id)
            .bind(&match_expr)
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query_as(&format!(
                "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent, {last_read} AS last_read_at FROM books b
                 {joins}
                 WHERE b.owner_id = $1
                 ORDER BY b.created_at DESC, b.id DESC"
            ))
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| internal(e.into()))?;

    // Shelf membership for the library's shelf filter and row chips: one
    // extra query, distributed here, rather than a dialect-specific aggregate.
    let pairs: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT sb.book_id, sb.shelf_id FROM shelf_books sb
         JOIN shelves s ON s.id = sb.shelf_id
         WHERE s.owner_id = $1
         ORDER BY sb.shelf_id",
    )
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let mut by_book: std::collections::HashMap<i64, Vec<i64>> = std::collections::HashMap::new();
    for (book_id, shelf_id) in pairs {
        by_book.entry(book_id).or_default().push(shelf_id);
    }
    let items = books
        .into_iter()
        .map(|book| {
            let shelf_ids = by_book.remove(&book.id).unwrap_or_default();
            BookListItem { book, shelf_ids }
        })
        .collect();
    Ok(Json(items))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<BookDetail>, Response> {
    let percent = progress::progress_percent(state.backend);
    let book: Option<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         {joins}
         WHERE b.owner_id = $1 AND b.id = $2",
        joins = progress::PROGRESS_JOINS,
    ))
    .bind(user.0.id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let book = book.ok_or_else(not_found)?;
    let shelves = book_shelves(&state, book.id).await?;
    let tags = book_tags(&state, book.id).await?;
    let federable = book.federable_status();
    let kobo_removed = kobo_removed(&state, book.id).await?;
    let health = stored_health(&state, book.id).await;
    let previous_in_series = series_neighbour(&state, user.0.id, &book, false).await?;
    let next_in_series = series_neighbour(&state, user.0.id, &book, true).await?;
    Ok(Json(BookDetail { book, shelves, tags, federable, kobo_removed, health, previous_in_series, next_in_series }))
}

#[derive(serde::Deserialize)]
pub struct UpdateBook {
    title: String,
    author: Option<String>,
    language: Option<String>,
    description: Option<String>,
    publisher: Option<String>,
    published: Option<String>,
    /// The year the work first appeared; absent = unchanged, null = clear.
    #[serde(default, deserialize_with = "deserialize_some")]
    first_published: Option<Option<i64>>,
    category: Option<String>,
    isbn: Option<String>,
    libris_id: Option<String>,
    series: Option<String>,
    series_index: Option<f64>,
    /// When present, replaces the book's tags.
    tags: Option<Vec<String>>,
    /// When present, replaces the book's shelf assignments.
    shelf_ids: Option<Vec<i64>>,
    /// When present, replaces the book's licence facts.
    license: Option<LicenseUpdate>,
}

#[derive(serde::Deserialize)]
pub struct LicenseUpdate {
    /// A name from license.rs, or null/empty for unknown.
    license: Option<String>,
    source_url: Option<String>,
    author_death_year: Option<i64>,
    #[serde(default)]
    cover_is_free: bool,
}

/// Checked licence facts, ready to store.
struct CheckedLicense {
    license: Option<&'static str>,
    source_url: Option<String>,
    author_death_year: Option<i64>,
    cover_is_free: bool,
}

fn check_license(req: LicenseUpdate) -> Result<CheckedLicense, &'static str> {
    let license = match clean(req.license) {
        None => None,
        Some(name) => Some(License::parse(&name).ok_or("unknown licence")?.as_str()),
    };
    let source_url = clean(req.source_url);
    if let Some(url) = &source_url {
        license::valid_source_url(url)?;
    }
    if let Some(year) = req.author_death_year {
        license::valid_death_year(year)?;
    }
    Ok(CheckedLicense { license, source_url, author_death_year: req.author_death_year, cover_is_free: req.cover_is_free })
}

/// Tells an absent field (None) from an explicit null (Some(None)).
fn deserialize_some<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

/// An optional float for binding. sqlx's Any driver declares a NULL f64 as
/// FLOAT4 on Postgres (Real and Double are swapped in its NULL mapping), and
/// the cached statement then rejects a real f64 with "incorrect binary data
/// format". Bind text and CAST in SQL instead: `CAST($n AS DOUBLE PRECISION)`.
pub(crate) fn float_param(value: Option<f64>) -> Option<String> {
    value.filter(|v| v.is_finite()).map(|v| v.to_string())
}

fn clean(value: Option<String>) -> Option<String> {
    value.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Pull an ISBN-10/13 out of an EPUB identifier like
/// "urn:isbn:978-91-0-012345-6" or a bare "9789100123456".
pub(crate) fn extract_isbn(identifier: &str) -> Option<String> {
    for part in identifier.split(|c: char| c == ',' || c == ';' || c == ' ') {
        let digits: String = part
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
            .collect();
        let valid = match digits.len() {
            13 => digits.chars().all(|c| c.is_ascii_digit()) && (digits.starts_with("978") || digits.starts_with("979")),
            10 => digits[..9].chars().all(|c| c.is_ascii_digit()),
            _ => false,
        };
        if valid {
            return Some(digits.to_uppercase());
        }
    }
    None
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateBook>,
) -> Result<Json<BookDetail>, Response> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "title must not be empty" })),
        )
            .into_response());
    }

    let unprocessable =
        |msg: &str| (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response();
    // This edition's date, normalized; anything unreadable is refused.
    let published = match clean(req.published.clone()) {
        None => None,
        Some(raw) => Some(crate::pubdate::normalize(&raw).ok_or_else(|| unprocessable("invalid published date"))?),
    };
    if let Some(Some(y)) = req.first_published {
        if !crate::pubdate::valid_first_year(y) {
            return Err(unprocessable("invalid first published year"));
        }
    }

    let license = match req.license.map(check_license).transpose() {
        Ok(l) => l,
        Err(msg) => {
            return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response())
        }
    };

    let mut tx = state.db.begin().await.map_err(|e| internal(e.into()))?;

    let federated_before: Vec<i64> = sqlx::query_scalar(
        "SELECT s.id FROM shelves s JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE sb.book_id = $1 AND s.visibility = 'federated' AND s.owner_id = $2",
    )
    .bind(id)
    .bind(user.0.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;

    if let Some(first) = req.first_published {
        // Text + CAST, as for every nullable number (see float_param).
        sqlx::query("UPDATE books SET first_published = CAST($1 AS BIGINT) WHERE id = $2 AND owner_id = $3")
            .bind(first.map(|y| y.to_string()))
            .bind(id)
            .bind(user.0.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| internal(e.into()))?;
    }

    if let Some(l) = &license {
        // The year travels as text + CAST: the Any driver's typing of a NULL
        // parameter is not to be trusted on Postgres (see float_param).
        sqlx::query(
            "UPDATE books SET license = $1, license_source_url = $2,
                 author_death_year = CAST($3 AS BIGINT), cover_is_free = $4
             WHERE id = $5 AND owner_id = $6",
        )
        .bind(l.license)
        .bind(&l.source_url)
        .bind(l.author_death_year.map(|y| y.to_string()))
        .bind(DbFlag::from(l.cover_is_free))
        .bind(id)
        .bind(user.0.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    }

    let book: Option<Book> = sqlx::query_as(&format!(
        "UPDATE books
         SET title = $1, author = $2, language = $3, description = $4, publisher = $5, published = $6, category = $7,
             isbn = $8, libris_id = $9, series = $10, series_index = CAST($11 AS DOUBLE PRECISION),
             updated_at = $12
         WHERE id = $13 AND owner_id = $14
         RETURNING {BOOK_COLUMNS}"
    ))
    .bind(title)
    .bind(clean(req.author))
    .bind(clean(req.language))
    .bind(clean(req.description))
    .bind(clean(req.publisher))
    .bind(&published)
    .bind(clean(req.category))
    .bind(clean(req.isbn))
    .bind(clean(req.libris_id))
    .bind(clean(req.series.clone()))
    .bind(float_param(if clean(req.series.clone()).is_some() { req.series_index } else { None }))
    .bind(now_ts())
    .bind(id)
    .bind(user.0.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    let mut book = book.ok_or_else(not_found)?;

    if let Some(tags) = &req.tags {
        sqlx::query("DELETE FROM book_tags WHERE book_id = $1")
            .bind(book.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| internal(e.into()))?;
        let mut seen: Vec<String> = Vec::new();
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() || tag.len() > 100 || seen.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                continue;
            }
            seen.push(tag.to_string());
            sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES ($1, $2) ON CONFLICT (book_id, tag) DO NOTHING")
                .bind(book.id)
                .bind(tag)
                .execute(&mut *tx)
                .await
                .map_err(|e| internal(e.into()))?;
        }
    }

    if let Some(shelf_ids) = req.shelf_ids {
        // Touch every shelf the book leaves or joins, so Kobo collections resync.
        sqlx::query(
            "UPDATE shelves SET updated_at = $1
             WHERE owner_id = $2 AND id IN (SELECT shelf_id FROM shelf_books WHERE book_id = $3)",
        )
        .bind(now_ts())
        .bind(user.0.id)
        .bind(book.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
        sqlx::query("DELETE FROM shelf_books WHERE book_id = $1")
            .bind(book.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| internal(e.into()))?;
        for shelf_id in shelf_ids {
            // Only attach shelves the user actually owns.
            sqlx::query(
                "INSERT INTO shelf_books (shelf_id, book_id)
                 SELECT id, $1 FROM shelves WHERE id = $2 AND owner_id = $3
                 ON CONFLICT (shelf_id, book_id) DO NOTHING",
            )
            .bind(book.id)
            .bind(shelf_id)
            .bind(user.0.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| internal(e.into()))?;
        }
        sqlx::query(
            "UPDATE shelves SET updated_at = $1
             WHERE owner_id = $2 AND id IN (SELECT shelf_id FROM shelf_books WHERE book_id = $3)",
        )
        .bind(now_ts())
        .bind(user.0.id)
        .bind(book.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| internal(e.into()))?;
    }

    // A book on a federated shelf must stay federable, and only a federable
    // book may join one. Returning here drops the transaction, so none of
    // the edit is kept.
    let federated_now: Vec<(i64, String)> = sqlx::query_as(
        "SELECT s.id, s.name FROM shelves s JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE sb.book_id = $1 AND s.visibility = 'federated'",
    )
    .bind(book.id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| internal(e.into()))?;
    if !federated_now.is_empty() {
        if let Err(reason) = license::federable(&book.license_facts(), license::current_year()) {
            let joined = federated_now.iter().any(|(sid, _)| !federated_before.contains(sid));
            return Err(if joined {
                crate::shelves::not_federable(vec![serde_json::json!({ "id": book.id, "title": book.title, "reason": reason })])
            } else {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({
                        "error": "book on federated shelf",
                        "shelves": federated_now.iter().map(|(_, n)| n).collect::<Vec<_>>(),
                        "reason": reason,
                    })),
                )
                    .into_response()
            });
        }
    }

    tx.commit().await.map_err(|e| internal(e.into()))?;
    state.fed.wake.notify_one();
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "book.edited",
        serde_json::json!({ "book_id": book.id, "title": book.title, "license": license.as_ref().map(|l| l.license) }),
    )
    .await;

    // The file follows the catalog. A file that cannot be rewritten does not
    // undo the edit; the health check reports on it.
    match tend_file(&state, &book, false).await {
        Ok(true) => {
            book = sqlx::query_as(&format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = $1"))
                .bind(book.id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
            state.fed.wake.notify_one();
        }
        Ok(false) => {}
        Err(e) => tracing::warn!("could not write metadata into {}: {e:#}", book.uuid),
    }

    book.progress_percent = progress::percent_for(&state, user.0.id, book.id)
        .await
        .map_err(|e| internal(e.into()))?;
    let shelves = book_shelves(&state, book.id).await?;
    let tags = book_tags(&state, book.id).await?;
    let federable = book.federable_status();
    let kobo_removed = kobo_removed(&state, book.id).await?;
    let health = stored_health(&state, book.id).await;
    let previous_in_series = series_neighbour(&state, user.0.id, &book, false).await?;
    let next_in_series = series_neighbour(&state, user.0.id, &book, true).await?;
    Ok(Json(BookDetail { book, shelves, tags, federable, kobo_removed, health, previous_in_series, next_in_series }))
}

#[derive(serde::Deserialize)]
pub struct SetWant {
    want: bool,
}

/// Put a book on, or take it off, the owner's want-to-read list.
pub async fn set_want(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<SetWant>,
) -> Result<StatusCode, Response> {
    let n = set_want_for(&state, user.0.id, &[id], req.want).await?;
    if n == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Returns how many of the books are the caller's (and now in the wanted state).
/// Two statements rather than a nullable bind (see set_rating).
async fn set_want_for(state: &AppState, user_id: i64, ids: &[i64], want: bool) -> Result<u64, Response> {
    let mut n = 0;
    for id in ids {
        if want {
            // wanted_at only when newly added, so the list keeps its order.
            sqlx::query("UPDATE books SET want_to_read = 1, wanted_at = $1 WHERE id = $2 AND owner_id = $3 AND want_to_read = 0")
                .bind(now_ts())
                .bind(id)
                .bind(user_id)
                .execute(&state.db)
                .await
        } else {
            sqlx::query("UPDATE books SET want_to_read = 0, wanted_at = NULL WHERE id = $1 AND owner_id = $2")
                .bind(id)
                .bind(user_id)
                .execute(&state.db)
                .await
        }
        .map_err(|e| internal(e.into()))?;
        let mine: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        n += mine as u64;
    }
    Ok(n)
}

/// What to do with a selection of books (the library's selection mode).
#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BulkAction {
    AddToShelf { shelf_id: i64 },
    RemoveFromShelf { shelf_id: i64 },
    AddTag { tag: String },
    RemoveTag { tag: String },
    Want { want: bool },
    /// Mend the files and write the catalog's metadata and cover into them.
    Repair,
    Delete,
}

#[derive(serde::Deserialize)]
pub struct BulkRequest {
    ids: Vec<i64>,
    #[serde(flatten)]
    action: BulkAction,
}

#[derive(Serialize)]
pub struct BulkResult {
    /// Books the action was applied to.
    done: usize,
    /// Books left out, with why (not federable for a federated shelf).
    skipped: Vec<serde_json::Value>,
}

/// One action on many of the caller's books. Books that aren't the caller's
/// are ignored. The federation gate holds: a book that may not federate is
/// skipped, and reported, rather than added to a federated shelf.
pub async fn bulk(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<BulkRequest>,
) -> Result<Json<BulkResult>, Response> {
    let unprocessable =
        |msg: &str| (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response();
    if req.ids.is_empty() || req.ids.len() > 5000 {
        return Err(unprocessable("select between 1 and 5000 books"));
    }
    let mut mine: Vec<Book> = Vec::new();
    for id in &req.ids {
        let book: Option<Book> = sqlx::query_as(&format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = $1 AND owner_id = $2"))
            .bind(id)
            .bind(user.0.id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        if let Some(b) = book {
            if !mine.iter().any(|m| m.id == b.id) {
                mine.push(b);
            }
        }
    }
    let mut done = 0;
    let mut skipped = Vec::new();
    let action_name: &str;
    let details: serde_json::Value;

    match &req.action {
        BulkAction::AddToShelf { shelf_id } | BulkAction::RemoveFromShelf { shelf_id } => {
            let shelf: Option<(String, String)> =
                sqlx::query_as("SELECT name, visibility FROM shelves WHERE id = $1 AND owner_id = $2")
                    .bind(shelf_id)
                    .bind(user.0.id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(|e| internal(e.into()))?;
            let (name, visibility) = shelf.ok_or_else(not_found)?;
            let add = matches!(req.action, BulkAction::AddToShelf { .. });
            for b in &mine {
                if add && visibility == "federated" {
                    if let Err(reason) = license::federable(&b.license_facts(), license::current_year()) {
                        skipped.push(serde_json::json!({ "id": b.id, "title": b.title, "reason": reason }));
                        continue;
                    }
                }
                let q = if add {
                    "INSERT INTO shelf_books (shelf_id, book_id) VALUES ($1, $2) ON CONFLICT (shelf_id, book_id) DO NOTHING"
                } else {
                    "DELETE FROM shelf_books WHERE shelf_id = $1 AND book_id = $2"
                };
                sqlx::query(q).bind(shelf_id).bind(b.id).execute(&state.db).await.map_err(|e| internal(e.into()))?;
                done += 1;
            }
            // Kobo collections resync from the shelf's updated_at.
            sqlx::query("UPDATE shelves SET updated_at = $1 WHERE id = $2")
                .bind(now_ts())
                .bind(shelf_id)
                .execute(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
            action_name = if add { "book.bulk_shelved" } else { "book.bulk_unshelved" };
            details = serde_json::json!({ "count": done, "shelf_id": shelf_id, "name": name });
        }
        BulkAction::AddTag { tag } | BulkAction::RemoveTag { tag } => {
            let tag = tag.trim();
            if tag.is_empty() || tag.len() > 100 {
                return Err(unprocessable("tag must be 1-100 characters"));
            }
            let add = matches!(req.action, BulkAction::AddTag { .. });
            for b in &mine {
                if add {
                    // Case-insensitively one tag per book, as in update().
                    let exists: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE book_id = $1 AND LOWER(tag) = LOWER($2)")
                            .bind(b.id)
                            .bind(tag)
                            .fetch_one(&state.db)
                            .await
                            .map_err(|e| internal(e.into()))?;
                    if exists == 0 {
                        sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES ($1, $2) ON CONFLICT (book_id, tag) DO NOTHING")
                            .bind(b.id)
                            .bind(tag)
                            .execute(&state.db)
                            .await
                            .map_err(|e| internal(e.into()))?;
                    }
                } else {
                    sqlx::query("DELETE FROM book_tags WHERE book_id = $1 AND LOWER(tag) = LOWER($2)")
                        .bind(b.id)
                        .bind(tag)
                        .execute(&state.db)
                        .await
                        .map_err(|e| internal(e.into()))?;
                }
                done += 1;
            }
            action_name = if add { "book.bulk_tagged" } else { "book.bulk_untagged" };
            details = serde_json::json!({ "count": done, "tag": tag });
        }
        BulkAction::Want { want } => {
            let ids: Vec<i64> = mine.iter().map(|b| b.id).collect();
            done = set_want_for(&state, user.0.id, &ids, *want).await? as usize;
            action_name = if *want { "book.bulk_wanted" } else { "book.bulk_unwanted" };
            details = serde_json::json!({ "count": done });
        }
        BulkAction::Repair => {
            for b in &mine {
                match tend_file(&state, b, true).await {
                    Ok(_) => done += 1,
                    Err(e) => tracing::warn!("could not repair {}: {e:#}", b.uuid),
                }
            }
            action_name = "book.bulk_repaired";
            details = serde_json::json!({ "count": done });
        }
        BulkAction::Delete => {
            for b in &mine {
                remove_book(&state, user.0.id, b.id, &b.uuid).await?;
                done += 1;
            }
            action_name = "book.bulk_deleted";
            details = serde_json::json!({
                "count": done,
                "titles": mine.iter().take(20).map(|b| b.title.clone()).collect::<Vec<_>>(),
            });
        }
    }
    state.fed.wake.notify_one();
    if done > 0 {
        crate::audit::log(&state, crate::audit::by(&user.0), action_name, details).await;
    }
    Ok(Json(BulkResult { done, skipped }))
}

#[derive(serde::Deserialize)]
pub struct ArchiveParams {
    /// Comma-separated book ids.
    ids: String,
}

/// Several books as one zip (stored, not compressed: EPUBs already are).
/// Built in a temp file under the data directory and streamed from there, so
/// a large selection never sits in memory.
pub async fn archive(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<ArchiveParams>,
) -> Result<Response, Response> {
    let ids: Vec<i64> = params.ids.split(',').filter_map(|s| s.trim().parse().ok()).take(5000).collect();
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut used = std::collections::HashSet::new();
    for id in ids {
        let row: Option<(String, String, Option<String>, String)> =
            sqlx::query_as("SELECT uuid, title, author, format FROM books WHERE id = $1 AND owner_id = $2")
                .bind(id)
                .bind(user.0.id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
        let Some((uuid, title, author, format)) = row else { continue };
        let format = Format::parse(&format);
        let raw = match &author {
            Some(a) => format!("{a} - {title}"),
            None => title,
        };
        let base: String = raw
            .chars()
            .map(|c| if c.is_alphanumeric() || " -.,'()".contains(c) { c } else { '_' })
            .take(150)
            .collect::<String>()
            .trim()
            .to_string();
        let ext = format.as_str();
        let mut name = format!("{base}.{ext}");
        let mut n = 2;
        while !used.insert(name.clone()) {
            name = format!("{base} ({n}).{ext}");
            n += 1;
        }
        files.push((name, book_path(&state, &uuid, format)));
    }
    if files.is_empty() {
        return Err(not_found());
    }
    let tmp_dir = state.data_dir.join("tmp");
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|e| internal(e.into()))?;
    let tmp = tmp_dir.join(format!("archive-{}.zip", new_uuid()));
    let target = tmp.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&target)?));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        for (name, path) in files {
            let Ok(mut f) = std::fs::File::open(&path) else { continue };
            zip.start_file(name, options)?;
            std::io::copy(&mut f, &mut zip)?;
        }
        zip.finish()?;
        Ok(())
    })
    .await
    .map_err(|e| internal(e.into()))?
    .map_err(internal)?;
    let file = tokio::fs::File::open(&tmp).await.map_err(|e| internal(e.into()))?;
    let len = file.metadata().await.map_err(|e| internal(e.into()))?.len();
    // Unlinked right away: the open handle keeps the data until the stream ends.
    let _ = tokio::fs::remove_file(&tmp).await;
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"legejo.zip\"".to_string()),
        ],
        body,
    )
        .into_response())
}

#[derive(serde::Deserialize)]
pub struct SetRating {
    rating: Option<i64>,
}

/// Rate a book 1–5, or clear the rating with null. Leaves updated_at alone:
/// a rating is not a metadata change the Kobo needs to resync for.
pub async fn set_rating(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<SetRating>,
) -> Result<StatusCode, Response> {
    // Two statements rather than one nullable bind: the Any driver's typing
    // of NULL parameters is not to be trusted on Postgres (see float_param).
    let result = match req.rating {
        Some(rating) if (1..=5).contains(&rating) => {
            sqlx::query("UPDATE books SET rating = $1 WHERE id = $2 AND owner_id = $3")
                .bind(rating)
                .bind(id)
                .bind(user.0.id)
                .execute(&state.db)
                .await
        }
        Some(_) => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "rating must be between 1 and 5" })),
            )
                .into_response())
        }
        None => {
            sqlx::query("UPDATE books SET rating = NULL WHERE id = $1 AND owner_id = $2")
                .bind(id)
                .bind(user.0.id)
                .execute(&state.db)
                .await
        }
    }
    .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn kobo_removed(state: &AppState, book_id: i64) -> Result<bool, Response> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books WHERE id = $1 AND kobo_removed_at IS NOT NULL")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(n > 0)
}

/// Send a book to the Kobo again after it was removed there. Clearing the
/// mark and bumping updated_at puts it in the next sync as a changed book;
/// its shelves are touched so the collections come along.
pub async fn kobo_restore(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    let title: Option<String> = sqlx::query_scalar(
        "UPDATE books SET kobo_removed_at = NULL, updated_at = $1
         WHERE id = $2 AND owner_id = $3 AND kobo_removed_at IS NOT NULL
         RETURNING title",
    )
    .bind(now_ts())
    .bind(id)
    .bind(user.0.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let title = title.ok_or_else(not_found)?;
    sqlx::query(
        "UPDATE shelves SET updated_at = $1
         WHERE owner_id = $2 AND id IN (SELECT shelf_id FROM shelf_books WHERE book_id = $3)",
    )
    .bind(now_ts())
    .bind(user.0.id)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    crate::audit::log(&state, crate::audit::by(&user.0), "kobo.restored", serde_json::json!({ "title": title })).await;
    Ok(StatusCode::NO_CONTENT)
}

struct ParsedEpub {
    title: String,
    author: Option<String>,
    language: Option<String>,
    description: Option<String>,
    publisher: Option<String>,
    published: Option<String>,
    identifier: Option<String>,
    cover: Option<(Vec<u8>, String)>,
    /// dc:subject entries become the book's initial tags.
    subjects: Vec<String>,
    series: Option<String>,
    series_index: Option<f64>,
    rating: Option<i64>,
}

fn parse_epub(bytes: &[u8], fallback_title: &str) -> anyhow::Result<ParsedEpub> {
    let mut doc = EpubDoc::from_reader(Cursor::new(bytes.to_vec()))
        .map_err(|e| anyhow::anyhow!("could not parse epub: {e}"))?;

    let metadata = doc.metadata.clone();
    let multi = |key: &str| -> Option<String> {
        let joined = metadata
            .iter()
            .filter(|item| item.property == key)
            .map(|item| item.value.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        (!joined.is_empty()).then_some(joined)
    };

    let title = multi("title").unwrap_or_else(|| fallback_title.to_string());
    let author = multi("creator");
    let language = multi("language");
    let description = multi("description");
    let publisher = multi("publisher");
    let published = multi("date").as_deref().and_then(crate::pubdate::normalize);
    let identifier = multi("identifier");
    let cover = doc.get_cover();

    let mut subjects: Vec<String> = Vec::new();
    for item in metadata.iter().filter(|item| item.property == "subject") {
        let tag = item.value.trim();
        if !tag.is_empty() && tag.len() <= 100 && !subjects.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            subjects.push(tag.to_string());
        }
    }
    subjects.truncate(20);

    let opf = opf_text(bytes);
    let (series, series_index) = opf.as_deref().map(scan_series).unwrap_or((None, None));
    let rating = opf.as_deref().and_then(scan_rating);

    Ok(ParsedEpub {
        title,
        author,
        language,
        description,
        publisher,
        published,
        identifier,
        cover,
        subjects,
        series,
        series_index,
        rating,
    })
}

/// The raw OPF document, for metadata the epub crate doesn't surface.
fn opf_text(bytes: &[u8]) -> Option<String> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).ok()?;
    let name = (0..archive.len()).find_map(|i| {
        let entry = archive.by_index_raw(i).ok()?;
        let n = entry.name().to_string();
        n.to_lowercase().ends_with(".opf").then_some(n)
    })?;
    let mut text = String::new();
    archive.by_name(&name).ok()?.read_to_string(&mut text).ok()?;
    Some(text)
}

/// The opening tag that contains `needle`, e.g. the <meta …> around a name.
fn tag_around<'a>(hay: &'a str, needle: &str) -> Option<&'a str> {
    let pos = hay.find(needle)?;
    let start = hay[..pos].rfind('<')?;
    let end = start + hay[start..].find('>')?;
    Some(&hay[start..end])
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let i = tag.find(&marker)? + marker.len();
    let j = i + tag[i..].find('\"')?;
    let v = tag[i..j].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// calibre's <meta name="calibre:rating" content="8"/>: a 0–10 scale where
/// 0 means unrated, halved here to 1–5.
fn scan_rating(opf: &str) -> Option<i64> {
    let value: f64 = tag_around(opf, "calibre:rating\"")
        .and_then(|tag| attr(tag, "content"))?
        .parse()
        .ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some(((value / 2.0).round() as i64).clamp(1, 5))
}

/// Series from either convention: calibre's EPUB2 metas
/// (<meta name="calibre:series" content="…"/>) or EPUB3's
/// belongs-to-collection with group-position.
fn scan_series(opf: &str) -> (Option<String>, Option<f64>) {
    fn element_text(hay: &str, needle: &str) -> Option<String> {
        let pos = hay.find(needle)?;
        let start = hay[..pos].rfind('<')?;
        let end = start + hay[start..].find('>')?;
        let rest = &hay[end + 1..];
        let close = rest.find('<')?;
        let v = rest[..close].trim();
        (!v.is_empty()).then(|| v.to_string())
    }

    let mut series = tag_around(opf, "calibre:series\"").and_then(|tag| attr(tag, "content"));
    let mut index = tag_around(opf, "calibre:series_index\"")
        .and_then(|tag| attr(tag, "content"))
        .and_then(|v| v.parse::<f64>().ok());

    if series.is_none() {
        series = element_text(opf, "belongs-to-collection\"");
        if series.is_some() && index.is_none() {
            index = element_text(opf, "group-position\"").and_then(|v| v.parse::<f64>().ok());
        }
    }
    // calibre writes series_index 1.0 even for books outside any series.
    let index = if series.is_some() { index } else { None };
    (series, index)
}

#[derive(Serialize)]
pub struct UploadResult {
    added: Vec<Book>,
    errors: Vec<String>,
    /// Files that match books already in the library.
    duplicates: Vec<Duplicate>,
    /// What the health check repaired and found in the added files that had
    /// something wrong with them.
    reports: Vec<UploadReport>,
}

#[derive(Serialize)]
pub struct UploadReport {
    filename: String,
    book_id: i64,
    #[serde(flatten)]
    health: Health,
}

/// `same_file`: byte-identical to an existing book, so it was NOT added
/// (unless ?allow_duplicates=1). `same_isbn`: a different file of what looks
/// like the same book (ISBN or identifier); it was added, and the owner is
/// told so they can compare.
#[derive(Serialize)]
pub struct Duplicate {
    filename: String,
    kind: &'static str,
    existing_id: i64,
    existing_title: String,
    /// The new book, for same_isbn.
    added_id: Option<i64>,
}

#[derive(serde::Deserialize)]
pub struct UploadParams {
    #[serde(default)]
    allow_duplicates: Option<String>,
}

pub async fn upload(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<UploadParams>,
    mut multipart: Multipart,
) -> Result<Json<UploadResult>, Response> {
    let mut added: Vec<Book> = Vec::new();
    let mut errors = Vec::new();
    let mut duplicates = Vec::new();
    let mut reports = Vec::new();
    let allow_duplicates = params.allow_duplicates.as_deref() == Some("1");

    let books_dir = state.data_dir.join("books");
    let covers_dir = state.data_dir.join("covers");
    tokio::fs::create_dir_all(&books_dir).await.map_err(|e| internal(e.into()))?;
    tokio::fs::create_dir_all(&covers_dir).await.map_err(|e| internal(e.into()))?;

    while let Some(field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let filename = field.file_name().unwrap_or("book").to_string();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{filename}: could not read the upload ({e})"));
                continue;
            }
        };
        if bytes.is_empty() {
            continue;
        }

        if !allow_duplicates {
            let sha = sha256_hex(&bytes);
            let same: Option<(i64, String)> =
                sqlx::query_as(
                    "SELECT id, title FROM books
                     WHERE owner_id = $1 AND (file_sha256 = $2 OR upload_sha256 = $2) ORDER BY id LIMIT 1",
                )
                    .bind(user.0.id)
                    .bind(&sha)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(|e| internal(e.into()))?;
            if let Some((existing_id, existing_title)) = same {
                duplicates.push(Duplicate { filename, kind: "same_file", existing_id, existing_title, added_id: None });
                continue;
            }
        }

        match store_book(&state, user.0.id, &bytes, &filename).await? {
            Ok(book) => {
                // Another file of the same book? Same ISBN, or same identifier.
                let similar: Option<(i64, String)> = sqlx::query_as(
                    "SELECT id, title FROM books
                     WHERE owner_id = $1 AND id <> $2
                       AND ((isbn IS NOT NULL AND isbn <> '' AND isbn = $3)
                            OR (identifier IS NOT NULL AND identifier <> '' AND identifier = $4))
                     ORDER BY id LIMIT 1",
                )
                .bind(user.0.id)
                .bind(book.id)
                .bind(book.isbn.as_deref().unwrap_or(""))
                .bind(book.identifier.as_deref().unwrap_or(""))
                .fetch_optional(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
                if let Some((existing_id, existing_title)) = similar {
                    duplicates.push(Duplicate {
                        filename: filename.clone(),
                        kind: "same_isbn",
                        existing_id,
                        existing_title,
                        added_id: Some(book.id),
                    });
                }
                crate::audit::log(&state, crate::audit::by(&user.0), "book.uploaded", serde_json::json!({ "book_id": book.id, "title": book.title })).await;
                if let Some(health) = stored_health(&state, book.id).await.filter(|h| !h.issues.is_empty() || !h.fixed.is_empty()) {
                    reports.push(UploadReport { filename: filename.clone(), book_id: book.id, health });
                }
                added.push(book)
            }
            Err(e) => errors.push(format!("{filename}: {e}")),
        }
    }

    Ok(Json(UploadResult { added, errors, duplicates, reports }))
}

/// A PDF or a comic archive as a book: what the file says about itself, with
/// the file's name as the title when it says nothing.
fn parse_other(format: Format, bytes: &[u8], fallback_title: &str) -> Result<ParsedEpub, String> {
    let found = match format {
        Format::Pdf => crate::formats::inspect_pdf(bytes),
        _ => crate::formats::inspect_cbz(bytes),
    }
    .map_err(str::to_string)?;
    Ok(ParsedEpub {
        title: found.title.unwrap_or_else(|| fallback_title.to_string()),
        author: found.author,
        language: found.language,
        description: found.description,
        publisher: found.publisher,
        published: found.published,
        identifier: None,
        cover: found.cover,
        subjects: found.subjects,
        series: found.series,
        series_index: found.series_index,
        rating: None,
    })
}

/// Store one file as a new book of `owner_id`: file, cover, metadata, tags.
/// In an EPUB, what can be repaired without asking is repaired first, so the
/// stored file may differ from the uploaded one; a PDF or a comic archive is
/// stored as it is. Ok(Err(..)) when the file is not a readable book.
pub(crate) async fn store_book(
    state: &AppState,
    owner_id: i64,
    bytes: &[u8],
    filename: &str,
) -> Result<Result<Book, String>, Response> {
    let books_dir = state.data_dir.join("books");
    let covers_dir = state.data_dir.join("covers");
    tokio::fs::create_dir_all(&books_dir).await.map_err(|e| internal(e.into()))?;
    tokio::fs::create_dir_all(&covers_dir).await.map_err(|e| internal(e.into()))?;
    let Some(format) = Format::detect(bytes, filename) else {
        return Ok(Err("not an EPUB, PDF or CBZ file".to_string()));
    };
    let fallback_title = crate::formats::stem(filename).to_string();
    let upload_sha256 = sha256_hex(bytes);
    if format != Format::Epub {
        // Reading a large PDF takes a while; keep it off the request threads.
        let (data, title) = (bytes.to_vec(), fallback_title.clone());
        let parsed = tokio::task::spawn_blocking(move || parse_other(format, &data, &title))
            .await
            .map_err(|e| internal(e.into()))?;
        return match parsed {
            Ok(parsed) => insert_book(state, owner_id, bytes, format, parsed, None, &upload_sha256).await.map(Ok),
            Err(e) => Ok(Err(e)),
        };
    }
    // A copy-protected file cannot be read here or sent on to a device.
    if epubfix::encrypted(bytes) {
        return Ok(Err("copy-protected".to_string()));
    }
    // A repair is kept only when the result still reads as a book.
    let repaired = match epubfix::repair(bytes) {
        Ok((Some(out), fixed)) if parse_epub(&out, &fallback_title).is_ok() => Some((out, fixed)),
        Ok(_) => None,
        Err(e) => {
            tracing::debug!("no repair of {filename}: {e:#}");
            None
        }
    };
    let (bytes, fixed) = match &repaired {
        Some((out, fixed)) => (out.as_slice(), fixed.clone()),
        None => (bytes, Vec::new()),
    };
    let parsed = match parse_epub(bytes, &fallback_title) {
        Ok(p) => p,
        Err(e) => return Ok(Err(e.to_string())),
    };
    let health = epubfix::inspect(bytes).ok().map(|issues| Health { v: epubfix::CHECK_VERSION, issues, fixed });
    let mut book = insert_book(state, owner_id, bytes, format, parsed, health, &upload_sha256).await?;
    if epubfix::claims_copyright(bytes) {
        let _ = sqlx::query("UPDATE books SET license = $1 WHERE id = $2")
            .bind(License::Copyright.as_str())
            .bind(book.id)
            .execute(&state.db)
            .await;
        book.license = Some(License::Copyright.as_str().to_string());
    }
    Ok(Ok(book))
}

/// The row, the file, the cover and the tags of a new book.
async fn insert_book(
    state: &AppState,
    owner_id: i64,
    bytes: &[u8],
    format: Format,
    parsed: ParsedEpub,
    health: Option<Health>,
    upload_sha256: &str,
) -> Result<Book, Response> {
    let covers_dir = state.data_dir.join("covers");
    let uuid = new_uuid();
    let file_size = bytes.len() as i64;
    let sha256 = sha256_hex(bytes);
    tokio::fs::write(book_path(state, &uuid, format), bytes)
        .await
        .map_err(|e| internal(e.into()))?;

    // Some EPUBs declare their cover XHTML page as the cover item; only a
    // real image is worth storing.
    let cover_mime = match &parsed.cover {
        Some((_, mime)) if !mime.starts_with("image/") => None,
        Some((data, mime)) => {
            tokio::fs::write(covers_dir.join(&uuid), data)
                .await
                .map_err(|e| internal(e.into()))?;
            Some(mime.clone())
        }
        None => None,
    };

    let isbn = parsed.identifier.as_deref().and_then(extract_isbn);
    let book: Book = sqlx::query_as(&format!(
        "INSERT INTO books (uuid, owner_id, title, author, language, description, publisher, published, identifier, isbn, file_size, cover_mime, series, series_index, file_sha256, upload_sha256, format)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, CAST($14 AS DOUBLE PRECISION), $15, $16, $17)
         RETURNING {BOOK_COLUMNS}"
    ))
    .bind(&uuid)
    .bind(owner_id)
    .bind(&parsed.title)
    .bind(&parsed.author)
    .bind(&parsed.language)
    .bind(&parsed.description)
    .bind(&parsed.publisher)
    .bind(&parsed.published)
    .bind(&parsed.identifier)
    .bind(&isbn)
    .bind(file_size)
    .bind(&cover_mime)
    .bind(&parsed.series)
    .bind(float_param(parsed.series_index))
    .bind(&sha256)
    .bind(upload_sha256)
    .bind(format.as_str())
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let mut book = book;
    if let Some(health) = &health {
        save_health(state, book.id, health).await;
        book.health_issues = health.attention();
    }
    if let Some(rating) = parsed.rating {
        sqlx::query("UPDATE books SET rating = $1 WHERE id = $2")
            .bind(rating)
            .bind(book.id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        book.rating = Some(rating);
    }

    for tag in &parsed.subjects {
        sqlx::query("INSERT INTO book_tags (book_id, tag) VALUES ($1, $2) ON CONFLICT (book_id, tag) DO NOTHING")
            .bind(book.id)
            .bind(tag)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    }

    crate::kosync::set_md5(state, book.id, &book.uuid).await;
    Ok(book)
}

#[derive(serde::Deserialize)]
pub struct AuthorParams {
    q: Option<String>,
}

/// Author suggestions across all users' books, most common first. Sharing
/// suggestions between users keeps author spellings consistent.
pub async fn authors(
    State(state): State<AppState>,
    _user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<AuthorParams>,
) -> Result<Json<Vec<String>>, Response> {
    let q = params.q.as_deref().unwrap_or("").trim().to_string();
    if q.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let authors: Vec<String> = sqlx::query_scalar(
        "SELECT author FROM books
         WHERE author IS NOT NULL AND LOWER(author) LIKE '%' || LOWER($1) || '%'
         GROUP BY author
         ORDER BY COUNT(*) DESC, LOWER(author)
         LIMIT 10",
    )
    .bind(&q)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(authors))
}

/// All of the caller's books in a series, reading order first.
pub async fn series(
    State(state): State<AppState>,
    user: AuthUser,
    Path(name): Path<String>,
) -> Result<Json<Vec<Book>>, Response> {
    let books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         {joins}
         WHERE b.owner_id = $1 AND b.series IS NOT NULL AND LOWER(b.series) = LOWER($2)
         ORDER BY CASE WHEN b.series_index IS NULL THEN 1 ELSE 0 END, b.series_index, LOWER(b.title)",
        percent = progress::progress_percent(state.backend),
        joins = progress::PROGRESS_JOINS,
    ))
    .bind(user.0.id)
    .bind(&name)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(books))
}

/// (uuid, cover_mime, title) of a book the user owns.
async fn owned_book(
    state: &AppState,
    user_id: i64,
    book_id: i64,
) -> Result<(String, Option<String>, String), Response> {
    let row: Option<(String, Option<String>, String)> =
        sqlx::query_as("SELECT uuid, cover_mime, title FROM books WHERE id = $1 AND owner_id = $2")
            .bind(book_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    row.ok_or_else(not_found)
}

/// Grid thumbnails: covers are stored as uploaded (often 1500 px wide and
/// several hundred kB), far more than a 150 px card needs.
const THUMB_WIDTH: u32 = 400;
const THUMB_HEIGHT: u32 = 800;

fn thumb_path(state: &AppState, uuid: &str) -> std::path::PathBuf {
    state.data_dir.join("thumbs").join(format!("{uuid}.jpg"))
}

fn make_thumbnail(src: &std::path::Path, dst: &std::path::Path) -> anyhow::Result<()> {
    // Cover files carry no extension, so the format is sniffed from the bytes.
    let img = image::ImageReader::open(src)?.with_guessed_format()?.decode()?;
    // Only ever scale down: a small original is re-encoded at its own size.
    let thumb = if img.width() > THUMB_WIDTH || img.height() > THUMB_HEIGHT {
        img.thumbnail(THUMB_WIDTH, THUMB_HEIGHT)
    } else {
        img
    }
    .to_rgb8();
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Unique temp name + rename: concurrent requests never see a half file.
    let tmp = dst.with_extension(format!("{}.tmp", new_uuid()));
    let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
    thumb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 75))?;
    drop(out);
    std::fs::rename(&tmp, dst)?;
    Ok(())
}

/// The thumbnail for a cover, generated on first use. None when the cover
/// can't be decoded; callers then serve the original.
pub(crate) async fn ensure_thumb(state: &AppState, uuid: &str) -> Option<std::path::PathBuf> {
    let dst = thumb_path(state, uuid);
    if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
        return Some(dst);
    }
    // Decoding a large cover takes tens of MB; the gate keeps a page full of
    // missing thumbnails from decoding them all at once.
    let _permit = state.thumb_gate.acquire().await.ok()?;
    if tokio::fs::try_exists(&dst).await.unwrap_or(false) {
        return Some(dst);
    }
    let src = state.data_dir.join("covers").join(uuid);
    let target = dst.clone();
    match tokio::task::spawn_blocking(move || make_thumbnail(&src, &target)).await {
        Ok(Ok(())) => Some(dst),
        Ok(Err(e)) => {
            tracing::warn!("thumbnail for {uuid} failed: {e:#}");
            None
        }
        Err(e) => {
            tracing::warn!("thumbnail task for {uuid} failed: {e}");
            None
        }
    }
}

pub(crate) async fn drop_thumb(state: &AppState, uuid: &str) {
    let _ = tokio::fs::remove_file(thumb_path(state, uuid)).await;
}

/// Generate thumbnails for covers that lack one, one at a time, so the
/// first visit after an upgrade doesn't pay for all of them.
pub async fn backfill_thumbs(state: AppState) {
    let uuids: Vec<String> =
        match sqlx::query_scalar("SELECT uuid FROM books WHERE cover_mime LIKE 'image/%'")
            .fetch_all(&state.db)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!("thumbnail backfill skipped: {e}");
                return;
            }
        };
    let mut made = 0;
    for uuid in &uuids {
        if !tokio::fs::try_exists(thumb_path(&state, uuid)).await.unwrap_or(false)
            && ensure_thumb(&state, uuid).await.is_some()
        {
            made += 1;
        }
    }
    if made > 0 {
        tracing::info!("generated {made} cover thumbnails");
    }
}

#[derive(serde::Deserialize)]
pub struct CoverParams {
    /// "thumb" for the grid-sized version.
    size: Option<String>,
    /// Cache-buster the frontend derives from updated_at.
    v: Option<String>,
}

/// The cover (or its thumbnail) for a book the caller is allowed to see.
pub(crate) async fn cover_response(
    state: &AppState,
    uuid: &str,
    mime: String,
    params: &CoverParams,
) -> Result<Response, Response> {
    // A versioned URL changes whenever the cover does, so it can be cached
    // for good; an unversioned one keeps the one-day lifetime.
    let cache = if params.v.is_some() {
        "private, max-age=31536000, immutable"
    } else {
        "private, max-age=86400"
    };
    if params.size.as_deref() == Some("thumb") {
        if let Some(path) = ensure_thumb(state, uuid).await {
            if let Ok(data) = tokio::fs::read(path).await {
                return Ok((
                    [(header::CONTENT_TYPE, "image/jpeg".to_string()), (header::CACHE_CONTROL, cache.into())],
                    data,
                )
                    .into_response());
            }
        }
    }
    let data = tokio::fs::read(state.data_dir.join("covers").join(uuid))
        .await
        .map_err(|_| not_found())?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache.into())], data).into_response())
}

pub async fn cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<CoverParams>,
) -> Result<Response, Response> {
    let (uuid, cover_mime, _) = owned_book(&state, user.0.id, id).await?;
    let mime = cover_mime.ok_or_else(not_found)?;
    cover_response(&state, &uuid, mime, &params).await
}

pub async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let (uuid, _, title) = owned_book(&state, user.0.id, id).await?;
    let (path, format) = book_file(&state, &uuid).await;
    let data = tokio::fs::read(path).await.map_err(|e| internal(e.into()))?;
    Ok((
        [
            (header::CONTENT_TYPE, format.mime().to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", download_name(&title, format))),
        ],
        data,
    )
        .into_response())
}

/// Mail servers and Amazon both cap the size of a message; an attachment
/// grows by a third on the way.
const KINDLE_MAX_BYTES: u64 = 25 * 1024 * 1024;
/// Books one user may send in an hour.
const KINDLE_PER_HOUR: i64 = 30;

/// Mail the book's file to the user's Kindle address. Amazon delivers it to
/// the device once the sending address is on the user's approved list.
pub async fn send_to_kindle(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    let fail = |status: StatusCode, code: &str| (status, Json(serde_json::json!({ "error": code }))).into_response();
    let (uuid, _, title) = owned_book(&state, user.0.id, id).await?;
    let Some(mail) = state.mail.as_ref() else {
        return Err(fail(StatusCode::CONFLICT, "no-mail"));
    };
    let address: Option<String> = sqlx::query_scalar("SELECT kindle_email FROM users WHERE id = $1")
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?
        .flatten();
    let Some(address) = address else {
        return Err(fail(StatusCode::CONFLICT, "no-address"));
    };
    let since = crate::db::ts_in_hours(-1);
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_log WHERE actor_id = $1 AND action = 'book.sent_to_kindle' AND at > $2",
    )
    .bind(user.0.id)
    .bind(since)
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if recent >= KINDLE_PER_HOUR {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "too-many"));
    }
    let (path, format) = book_file(&state, &uuid).await;
    // Amazon takes EPUB and PDF by mail, not comic archives.
    if format == Format::Cbz {
        return Err(fail(StatusCode::CONFLICT, "not-supported"));
    }
    let size = tokio::fs::metadata(&path).await.map_err(|e| internal(e.into()))?.len();
    if size > KINDLE_MAX_BYTES {
        return Err(fail(StatusCode::PAYLOAD_TOO_LARGE, "too-large"));
    }
    let data = tokio::fs::read(&path).await.map_err(|e| internal(e.into()))?;
    // A short, plain file name: one with other characters is sent in an
    // encoding that not every mail reader understands. The Kindle shows the
    // title from inside the book.
    let name: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .take(50)
        .collect();
    let name = if name.trim_matches(['_', ' ']).is_empty() { "book" } else { name.trim() };
    let filename = format!("{name}.{}", format.as_str());
    if let Err(e) = crate::mail::send_file(mail, &address, &title, &title, &filename, format.mime(), data).await {
        tracing::warn!("send to kindle: {e:#}");
        return Err(fail(StatusCode::BAD_GATEWAY, "mail-failed"));
    }
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "book.sent_to_kindle",
        serde_json::json!({ "book_id": id, "title": title }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    let (uuid, _, title) = owned_book(&state, user.0.id, id).await?;
    remove_book(&state, user.0.id, id, &uuid).await?;
    state.fed.wake.notify_one();
    crate::audit::log(&state, crate::audit::by(&user.0), "book.deleted", serde_json::json!({ "book_id": id, "title": title })).await;
    Ok(StatusCode::NO_CONTENT)
}

/// The row and every file that belongs to the book.
pub(crate) async fn remove_book(state: &AppState, owner_id: i64, id: i64, uuid: &str) -> Result<(), Response> {
    sqlx::query("DELETE FROM books WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(owner_id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    // The row is gone, and with it the format: one of these is the file.
    for format in [Format::Epub, Format::Pdf, Format::Cbz] {
        let _ = tokio::fs::remove_file(book_path(state, uuid, format)).await;
    }
    let _ = tokio::fs::remove_file(state.data_dir.join("covers").join(uuid)).await;
    let _ = tokio::fs::remove_file(state.data_dir.join("kepub").join(format!("{uuid}.kepub.epub"))).await;
    drop_thumb(state, uuid).await;
    Ok(())
}

const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;

#[derive(Serialize)]
pub struct CoverResult {
    #[serde(flatten)]
    pub book: Book,
    /// Whether the new cover was also written into the EPUB file itself.
    pub epub_updated: bool,
}

pub async fn upload_cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<CoverResult>, Response> {
    let (uuid, _, _) = owned_book(&state, user.0.id, id).await?;

    let mut uploaded: Option<(Vec<u8>, String)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let mime = field.content_type().unwrap_or("").to_string();
        if !mime.starts_with("image/") {
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| internal(e.into()))?;
        if bytes.len() > MAX_COVER_BYTES {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": "image too large (max 10 MB)" })),
            )
                .into_response());
        }
        uploaded = Some((bytes.to_vec(), mime));
        break;
    }
    let (bytes, mime) = uploaded.ok_or_else(|| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "no image file in upload" })),
        )
            .into_response()
    })?;

    apply_cover(&state, &user, id, &uuid, bytes, mime).await
}

/// Store a new cover for one of the user's books: catalog file, thumbnail,
/// the image inside the EPUB (best effort), and the row. Shared by upload
/// and the Open Library lookup.
pub(crate) async fn apply_cover(
    state: &AppState,
    user: &AuthUser,
    id: i64,
    uuid: &str,
    bytes: Vec<u8>,
    mime: String,
) -> Result<Json<CoverResult>, Response> {
    let state = state.clone();
    let uuid = uuid.to_string();
    // Update the catalog cover; the old thumbnail no longer matches.
    tokio::fs::write(state.data_dir.join("covers").join(&uuid), &bytes)
        .await
        .map_err(|e| internal(e.into()))?;
    drop_thumb(&state, &uuid).await;
    // A new cover is a new work: it is not free until the owner says so.
    sqlx::query(
        "UPDATE books SET cover_mime = $1, cover_is_free = 0, updated_at = $2 WHERE id = $3 AND owner_id = $4",
    )
        .bind(&mime)
        .bind(now_ts())
        .bind(id)
        .bind(user.0.id)
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    // Best effort: put the image into the EPUB as well. Other formats keep
    // their cover in the catalog only.
    let (epub_path, format) = book_file(&state, &uuid).await;
    let (path, image, image_mime) = (epub_path.clone(), bytes.clone(), mime.clone());
    let epub_updated = format == Format::Epub
        && tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let out = epubfix::set_cover(&std::fs::read(&path)?, &image, &image_mime)?;
            write_atomic(&path, &out)?;
            Ok(())
        })
        .await
        .map_err(|e| internal(e.into()))?
        .map_err(|e| tracing::warn!("could not embed cover in epub {uuid}: {e:#}"))
        .is_ok();
    if epub_updated {
        file_changed(&state, id, &uuid).await;
        refresh_health(&state, id, &uuid, &[]).await;
    }

    let mut book: Book = sqlx::query_as(&format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = $1"))
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    book.progress_percent = progress::percent_for(&state, user.0.id, id)
        .await
        .map_err(|e| internal(e.into()))?;
    state.fed.wake.notify_one();
    crate::audit::log(&state, crate::audit::by(&user.0), "book.cover", serde_json::json!({ "book_id": book.id, "title": book.title })).await;
    Ok(Json(CoverResult { book, epub_updated }))
}

/// Replace a file through a temporary one beside it, so that a failed write
/// never leaves half a book and two writers never share a temporary file.
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", new_uuid()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// After the stored file of a book changed: size, hash and KOReader id
/// follow, and the converted copy for Kobo is made again when next asked for.
async fn file_changed(state: &AppState, id: i64, uuid: &str) {
    let (path, _) = book_file(state, uuid).await;
    if let Ok(data) = tokio::fs::read(&path).await {
        let _ = sqlx::query("UPDATE books SET file_size = $1, file_sha256 = $2 WHERE id = $3")
            .bind(data.len() as i64)
            .bind(sha256_hex(&data))
            .bind(id)
            .execute(&state.db)
            .await;
    }
    let _ = tokio::fs::remove_file(state.data_dir.join("kepub").join(format!("{uuid}.kepub.epub"))).await;
    crate::kosync::set_md5(state, id, uuid).await;
}

pub(crate) async fn stored_health(state: &AppState, id: i64) -> Option<Health> {
    let raw: Option<String> = sqlx::query_scalar("SELECT health FROM books WHERE id = $1").bind(id).fetch_optional(&state.db).await.ok()??;
    serde_json::from_str(&raw?).ok()
}

async fn save_health(state: &AppState, id: i64, health: &Health) {
    let _ = sqlx::query("UPDATE books SET health = $1, health_issues = $2 WHERE id = $3")
        .bind(serde_json::to_string(health).unwrap_or_default())
        .bind(health.attention())
        .bind(id)
        .execute(&state.db)
        .await;
}

/// Run the health check on a book's file and store the result. `fixed` adds
/// to the repairs already noted for the book.
pub(crate) async fn refresh_health(state: &AppState, id: i64, uuid: &str, fixed: &[String]) {
    let (path, format) = book_file(state, uuid).await;
    // The check is one of EPUB files.
    if format != Format::Epub {
        return;
    }
    let issues = tokio::task::spawn_blocking(move || epubfix::inspect(&std::fs::read(path)?)).await;
    let Ok(Ok(issues)) = issues else { return };
    let mut health = stored_health(state, id).await.unwrap_or_default();
    health.v = epubfix::CHECK_VERSION;
    health.issues = issues;
    for code in fixed {
        if !health.fixed.contains(code) {
            health.fixed.push(code.clone());
        }
    }
    save_health(state, id, &health).await;
}

/// Bring a book's file in line with the catalog: title, authors, language
/// and series are written into it, the catalog's cover is put into a file
/// that has none, and with `repair` what the health check can mend is
/// mended. Returns whether the file changed.
pub(crate) async fn tend_file(state: &AppState, book: &Book, repair: bool) -> anyhow::Result<bool> {
    // Only an EPUB is written to; a PDF or a comic archive stays as it came.
    if Format::parse(&book.format) != Format::Epub {
        return Ok(false);
    }
    let path = book_path(state, &book.uuid, Format::Epub);
    let (title, author, language, series, series_index) =
        (book.title.clone(), book.author.clone(), book.language.clone(), book.series.clone(), book.series_index);
    // The catalog's cover, when it is an image a file can carry.
    let cover_mime: Option<String> =
        sqlx::query_scalar("SELECT cover_mime FROM books WHERE id = $1").bind(book.id).fetch_optional(&state.db).await?.flatten();
    let cover = cover_mime.filter(|m| m.starts_with("image/")).map(|mime| (state.data_dir.join("covers").join(&book.uuid), mime));
    let (changed, fixed) = tokio::task::spawn_blocking(move || -> anyhow::Result<(bool, Vec<String>)> {
        let original = std::fs::read(&path)?;
        let mut current = original.clone();
        let mut fixed = Vec::new();
        if repair {
            if let (Some(out), done) = epubfix::repair(&current)? {
                current = out;
                fixed = done;
            }
        }
        let meta = epubfix::Meta {
            title: &title,
            author: author.as_deref(),
            language: language.as_deref(),
            series: series.as_deref(),
            series_index,
        };
        if let Some(out) = epubfix::write_metadata(&current, &meta)? {
            current = out;
        }
        // A cover chosen in the catalog while the file had nowhere to put it.
        if let Some((path, mime)) = &cover {
            if !epubfix::has_cover(&current) {
                if let Ok(image) = std::fs::read(path) {
                    current = epubfix::set_cover(&current, &image, mime)?;
                    fixed.push("no_cover".into());
                }
            }
        }
        if current == original {
            return Ok((false, fixed));
        }
        // Whatever was changed, the result must still read as a book.
        EpubDoc::from_reader(Cursor::new(current.clone())).map_err(|e| anyhow::anyhow!("result does not parse: {e}"))?;
        write_atomic(&path, &current)?;
        Ok((true, fixed))
    })
    .await??;
    if changed {
        file_changed(state, book.id, &book.uuid).await;
    }
    if changed || repair {
        refresh_health(state, book.id, &book.uuid, &fixed).await;
    }
    Ok(changed)
}

/// POST /api/books/{id}/repair: mend what can be mended in the file and
/// write the catalog's metadata into it.
pub async fn repair(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<Json<BookDetail>, Response> {
    let book: Option<Book> = sqlx::query_as(&format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = $1 AND owner_id = $2"))
        .bind(id)
        .bind(user.0.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let book = book.ok_or_else(not_found)?;
    let changed = tend_file(&state, &book, true).await.map_err(|e| {
        tracing::warn!("could not repair {}: {e:#}", book.uuid);
        (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": "could not repair the file" }))).into_response()
    })?;
    if changed {
        state.fed.wake.notify_one();
        crate::audit::log(&state, crate::audit::by(&user.0), "book.repaired", serde_json::json!({ "book_id": book.id, "title": book.title })).await;
    }
    get_one(State(state), user, Path(id)).await
}

/// Run the health check on books that have not had one, or had one from an
/// older version of the check.
pub async fn backfill_health(state: AppState) {
    let current = format!("%\"v\":{},%", epubfix::CHECK_VERSION);
    let rows: Vec<(i64, String)> =
        match sqlx::query_as("SELECT id, uuid FROM books WHERE format = 'epub' AND (health IS NULL OR health NOT LIKE $1) ORDER BY id")
            .bind(current)
            .fetch_all(&state.db)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("health check backfill skipped: {e}");
                return;
            }
        };
    if rows.is_empty() {
        return;
    }
    let total = rows.len();
    for (id, uuid) in rows {
        refresh_health(&state, id, &uuid, &[]).await;
        // A book with no licence stated gets the one its file claims.
        let path = book_path(&state, &uuid, Format::Epub);
        let claimed = tokio::task::spawn_blocking(move || std::fs::read(path).map(|b| epubfix::claims_copyright(&b)).unwrap_or(false)).await;
        if claimed.unwrap_or(false) {
            let _ = sqlx::query("UPDATE books SET license = $1 WHERE id = $2 AND (license IS NULL OR license = '')")
                .bind(License::Copyright.as_str())
                .bind(id)
                .execute(&state.db)
                .await;
        }
    }
    tracing::info!("health check: {total} books checked");
}

/// Lowercase hex SHA-256, the form federation compares.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The same hash, streamed from disk: a book may be a few hundred MB.
fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

/// Hash the EPUBs that lack a file_sha256 (books from before the column),
/// one at a time and off the async threads.
pub async fn backfill_sha256(state: AppState) {
    let rows: Vec<(i64, String)> =
        match sqlx::query_as("SELECT id, uuid FROM books WHERE file_sha256 IS NULL").fetch_all(&state.db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("sha256 backfill skipped: {e}");
                return;
            }
        };
    let mut done = 0;
    for (id, uuid) in rows {
        let (path, _) = book_file(&state, &uuid).await;
        let hash = tokio::task::spawn_blocking(move || sha256_file(&path)).await;
        let Ok(Ok(hash)) = hash else {
            tracing::warn!("sha256 backfill: could not read the epub for {uuid}");
            continue;
        };
        if let Err(e) = sqlx::query("UPDATE books SET file_sha256 = $1 WHERE id = $2 AND file_sha256 IS NULL")
            .bind(&hash)
            .bind(id)
            .execute(&state.db)
            .await
        {
            tracing::warn!("sha256 backfill stopped: {e}");
            return;
        }
        done += 1;
    }
    if done > 0 {
        tracing::info!("hashed {done} epub files");
    }
}

pub(crate) fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibre_rating_is_halved() {
        let opf = |v: &str| format!(r#"<metadata><meta name="calibre:rating" content="{v}"/></metadata>"#);
        assert_eq!(scan_rating(&opf("8.00")), Some(4));
        assert_eq!(scan_rating(&opf("10")), Some(5));
        assert_eq!(scan_rating(&opf("1")), Some(1));
        assert_eq!(scan_rating(&opf("0.00")), None);
        assert_eq!(scan_rating(&opf("x")), None);
        assert_eq!(scan_rating("<metadata/>"), None);
    }
}
