//! Public shelves: any logged-in user can browse other users' non-private
//! shelves and import books from them. There is no license gate inside the
//! instance. An import copies the EPUB and cover on disk under a new uuid,
//! so the copy survives if the owner later deletes or edits theirs.

use crate::auth::AuthUser;
use crate::books::{new_uuid, Book, BOOK_COLUMNS_B};
use crate::db::DbFlag;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response()
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PublicShelf {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub has_cover: DbFlag,
    pub book_count: i64,
    pub owner: String,
    pub owner_id: i64,
    pub owner_has_avatar: DbFlag,
}

/// Public shelves belonging to OTHER users; your own live under My shelves.
pub async fn shelves(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<PublicShelf>>, Response> {
    let shelves: Vec<PublicShelf> = sqlx::query_as(
        "SELECT s.id, s.name, s.description,
                CAST(CASE WHEN s.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover,
                COUNT(sb.book_id) AS book_count,
                u.username AS owner, u.id AS owner_id,
                CAST(CASE WHEN u.avatar_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS owner_has_avatar
         FROM shelves s
         JOIN users u ON u.id = s.owner_id
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE s.visibility <> 'private' AND s.owner_id != $1
         GROUP BY s.id, u.id
         ORDER BY LOWER(s.name)",
    )
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(shelves))
}

/// True (1) when the caller already has this book: a copy imported from it,
/// a copy sharing the same origin, the original it was imported from, or a
/// separately uploaded copy with the same ISBN/identifier. `user_param` is
/// the placeholder bound to the caller's user id (it differs per query).
pub(crate) fn owned_expr(user_param: &str) -> String {
    format!(
        "CAST(CASE WHEN EXISTS (
    SELECT 1 FROM books mine WHERE mine.owner_id = {user_param} AND (
        mine.source_uuid = b.uuid
        OR (b.source_uuid IS NOT NULL AND mine.source_uuid = b.source_uuid)
        OR (b.source_uuid IS NOT NULL AND mine.uuid = b.source_uuid)
        OR (b.isbn IS NOT NULL AND b.isbn != '' AND mine.isbn = b.isbn)
        OR (b.identifier IS NOT NULL AND b.identifier != '' AND mine.identifier = b.identifier)
    )
) THEN 1 ELSE 0 END AS BIGINT)"
    )
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PublicBook {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub book: Book,
    /// Whether the requesting user already has this book in their library.
    pub owned: DbFlag,
}

#[derive(Serialize)]
pub struct PublicShelfDetail {
    #[serde(flatten)]
    pub shelf: PublicShelf,
    pub books: Vec<PublicBook>,
}

/// Owner username and shelf name identify a public shelf (both unique,
/// case-insensitively); the shelf URLs are built on them.
async fn fetch_public_shelf(state: &AppState, owner: &str, name: &str) -> Result<PublicShelf, Response> {
    let shelf: Option<PublicShelf> = sqlx::query_as(
        "SELECT s.id, s.name, s.description,
                CAST(CASE WHEN s.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover,
                COUNT(sb.book_id) AS book_count,
                u.username AS owner, u.id AS owner_id,
                CAST(CASE WHEN u.avatar_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS owner_has_avatar
         FROM shelves s
         JOIN users u ON u.id = s.owner_id
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE LOWER(u.username) = LOWER($1) AND LOWER(s.name) = LOWER($2) AND s.visibility <> 'private'
         GROUP BY s.id, u.id",
    )
    .bind(owner)
    .bind(name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    shelf.ok_or_else(not_found)
}

pub async fn shelf(
    State(state): State<AppState>,
    user: AuthUser,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<PublicShelfDetail>, Response> {
    let shelf = fetch_public_shelf(&state, &owner, &name).await?;
    let books: Vec<PublicBook> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {owned} AS owned FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1
         ORDER BY sb.added_at DESC, b.id DESC",
        owned = owned_expr("$2")
    ))
    .bind(shelf.id)
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(PublicShelfDetail { shelf, books }))
}

pub async fn shelf_cover(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let mime: Option<Option<String>> = sqlx::query_scalar(
        "SELECT cover_mime FROM shelves WHERE id = $1 AND visibility <> 'private'",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let mime = mime.flatten().ok_or_else(not_found)?;
    let data = tokio::fs::read(state.data_dir.join("shelf_covers").join(id.to_string()))
        .await
        .map_err(|_| not_found())?;
    Ok((
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, no-cache".into())],
        data,
    )
        .into_response())
}

/// A book is publicly visible when it sits on at least one public shelf.
async fn public_book(state: &AppState, book_id: i64) -> Result<Book, Response> {
    let book: Option<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b
         WHERE b.id = $1 AND EXISTS (
             SELECT 1 FROM shelf_books sb
             JOIN shelves s ON s.id = sb.shelf_id
             WHERE sb.book_id = b.id AND s.visibility <> 'private'
         )"
    ))
    .bind(book_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    book.ok_or_else(not_found)
}


#[derive(Serialize)]
pub struct ShelfBookDetail {
    pub shelf: PublicShelf,
    pub book: PublicBook,
}

/// One book on a specific public shelf, addressed by owner/shelf/uuid. The
/// shelf is returned too so the page can keep the navigation context.
pub async fn shelf_book(
    State(state): State<AppState>,
    user: AuthUser,
    Path((owner, name, uuid)): Path<(String, String, String)>,
) -> Result<Json<ShelfBookDetail>, Response> {
    let shelf = fetch_public_shelf(&state, &owner, &name).await?;
    let book: Option<PublicBook> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {owned} AS owned FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $3 AND b.uuid = $1",
        owned = owned_expr("$2")
    ))
    .bind(&uuid)
    .bind(user.0.id)
    .bind(shelf.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let book = book.ok_or_else(not_found)?;
    Ok(Json(ShelfBookDetail { shelf, book }))
}

pub async fn book_cover(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<crate::books::CoverParams>,
) -> Result<Response, Response> {
    let book = public_book(&state, id).await?;
    let mime: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM books WHERE id = $1")
        .bind(book.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let mime = mime.ok_or_else(not_found)?;
    crate::books::cover_response(&state, &book.uuid, mime, &params).await
}

/// Copy a publicly visible book into the caller's library: new uuid, the
/// EPUB and cover duplicated on disk, metadata carried over. Reading
/// progress and shelf assignments do not follow.
pub async fn import_book(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<Book>), Response> {
    let source = public_book(&state, id).await?;
    if source_owner(&state, source.id).await? == user.0.id {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "the book is already yours" })),
        )
            .into_response());
    }
    let already: i64 = sqlx::query_scalar(&format!(
        "SELECT {owned} FROM books b WHERE b.id = $1",
        owned = owned_expr("$2"),
    ))
    .bind(source.id)
    .bind(user.0.id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if already != 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "you already have this book" })),
        )
            .into_response());
    }

    let uuid = new_uuid();
    let src_epub = state.data_dir.join("books").join(format!("{}.epub", source.uuid));
    let dst_epub = state.data_dir.join("books").join(format!("{uuid}.epub"));
    tokio::fs::copy(&src_epub, &dst_epub).await.map_err(|e| internal(e.into()))?;

    let cover_mime: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM books WHERE id = $1")
        .bind(source.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if cover_mime.is_some() {
        let src_cover = state.data_dir.join("covers").join(&source.uuid);
        let dst_cover = state.data_dir.join("covers").join(&uuid);
        if tokio::fs::copy(&src_cover, &dst_cover).await.is_err() {
            tracing::warn!("import: cover file missing for {}", source.uuid);
        }
    }

    let book: Book = sqlx::query_as(&format!(
        "INSERT INTO books (uuid, owner_id, title, author, language, description, publisher,
                            published, identifier, category, isbn, libris_id, file_size, cover_mime,
                            source_uuid, series, series_index,
                            license, license_source_url, author_death_year, cover_is_free, file_sha256)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                 CAST($17 AS DOUBLE PRECISION), $18, $19, CAST($20 AS BIGINT), $21,
                 (SELECT file_sha256 FROM books WHERE id = $22))
         RETURNING {BOOK_COLUMNS}",
        BOOK_COLUMNS = crate::books::BOOK_COLUMNS,
    ))
    .bind(&uuid)
    .bind(user.0.id)
    .bind(&source.title)
    .bind(&source.author)
    .bind(&source.language)
    .bind(&source.description)
    .bind(&source.publisher)
    .bind(&source.published)
    .bind(&source.identifier)
    .bind(&source.category)
    .bind(&source.isbn)
    .bind(&source.libris_id)
    .bind(source.file_size)
    .bind(&cover_mime)
    .bind(source.source_uuid.as_deref().unwrap_or(&source.uuid))
    .bind(&source.series)
    .bind(crate::books::float_param(source.series_index))
    // The license is a fact about the work and travels with the copy; the
    // file is byte-identical, so is its hash.
    .bind(&source.license)
    .bind(&source.license_source_url)
    .bind(source.author_death_year.map(|y| y.to_string()))
    .bind(source.cover_is_free)
    .bind(source.id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let from: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = (SELECT owner_id FROM books WHERE id = $1)")
        .bind(source.id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    crate::kosync::set_md5(&state, book.id, &book.uuid).await;
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "book.imported",
        serde_json::json!({ "book_id": book.id, "title": book.title, "from": from }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(book)))
}

async fn source_owner(state: &AppState, book_id: i64) -> Result<i64, Response> {
    sqlx::query_scalar("SELECT owner_id FROM books WHERE id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))
}
