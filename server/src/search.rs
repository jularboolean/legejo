//! The unified search: one query over the caller's own library, books on
//! other users' public shelves, the caller's own shelves and the public
//! ones, and the caller's audiobooks.

use crate::auth::AuthUser;
use crate::books::{search_expr, search_parts, Book, BOOK_COLUMNS_B};
use crate::db::DbFlag;
use crate::progress;
use crate::public::{owned_expr, PublicShelf};
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const GROUP_LIMIT: i64 = 50;

/// SQL condition: the book `b` has a tag that contains $2, a LIKE pattern
/// whose wildcards are escaped.
const TAGGED: &str = "EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id = b.id \
     AND LOWER(bt.tag) LIKE '%' || LOWER($2) || '%' ESCAPE '\\')";

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

/// A book found on someone else's public shelf, with enough context to
/// link to /public/{owner}/{shelf_name}/{uuid}.
#[derive(Serialize, sqlx::FromRow)]
pub struct PublicHit {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub book: Book,
    pub owner: String,
    pub shelf_name: String,
    pub owned: DbFlag,
}

/// One of the caller's own shelves, found by its name.
#[derive(Serialize, sqlx::FromRow)]
pub struct OwnShelf {
    pub id: i64,
    pub name: String,
    pub has_cover: DbFlag,
    pub book_count: i64,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub mine: Vec<Book>,
    pub my_shelves: Vec<OwnShelf>,
    pub public: Vec<PublicHit>,
    pub shelves: Vec<PublicShelf>,
    pub audiobooks: Vec<crate::audiobooks::Audiobook>,
}

pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<Params>,
) -> Result<Json<SearchResult>, Response> {
    let raw = params.q.as_deref().unwrap_or("").trim().to_string();
    let audiobooks = crate::audiobooks::search(&state, user.0.id, &raw).await?;
    // Shelf names are short; a simple substring match beats FTS here.
    let my_shelves: Vec<OwnShelf> = if raw.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(&format!(
            "SELECT s.id, s.name,
                    CAST(CASE WHEN s.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover,
                    COUNT(sb.book_id) AS book_count
             FROM shelves s
             LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
             WHERE s.owner_id = $1 AND LOWER(s.name) LIKE '%' || LOWER($2) || '%'
             GROUP BY s.id
             ORDER BY LOWER(s.name)
             LIMIT {GROUP_LIMIT}"
        ))
        .bind(user.0.id)
        .bind(&raw)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?
    };
    let expr = search_expr(state.backend, &raw);
    let Some(expr) = expr else {
        return Ok(Json(SearchResult { mine: Vec::new(), my_shelves, public: Vec::new(), shelves: Vec::new(), audiobooks }));
    };
    let parts = search_parts(state.backend);

    // Both book queries bind ($1 = caller, $2 = match expression): the FTS
    // condition from search_parts hardcodes $2, owned_expr gets $1.
    let mine: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         {search_join}
         {progress_joins}
         WHERE b.owner_id = $1 AND {condition}
         ORDER BY {order}
         LIMIT {GROUP_LIMIT}",
        percent = progress::progress_percent(state.backend),
        progress_joins = progress::PROGRESS_JOINS,
        search_join = parts.join,
        condition = parts.condition,
        order = parts.order,
    ))
    .bind(user.0.id)
    .bind(&expr)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    // Tags are not part of the full-text index: books with a tag that
    // contains the query are found on their own and follow the others.
    let pattern = raw.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let tagged: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         {progress_joins}
         WHERE b.owner_id = $1 AND {TAGGED}
         ORDER BY LOWER(b.title)
         LIMIT {GROUP_LIMIT}",
        percent = progress::progress_percent(state.backend),
        progress_joins = progress::PROGRESS_JOINS,
    ))
    .bind(user.0.id)
    .bind(&pattern)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let mut mine = mine;
    for book in tagged {
        if !mine.iter().any(|b| b.id == book.id) {
            mine.push(book);
        }
    }

    let public: Vec<PublicHit> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, u.username AS owner, ps.name AS shelf_name, {owned} AS owned
         FROM books b
         {search_join}
         JOIN (
             SELECT sb.book_id, MIN(s.id) AS shelf_id
             FROM shelf_books sb
             JOIN shelves s ON s.id = sb.shelf_id
             WHERE {visible} AND s.owner_id != $1
             GROUP BY sb.book_id
         ) pick ON pick.book_id = b.id
         JOIN shelves ps ON ps.id = pick.shelf_id
         JOIN users u ON u.id = ps.owner_id
         WHERE {condition}
         ORDER BY {order}
         LIMIT {GROUP_LIMIT}",
        owned = owned_expr("$1"),
        visible = crate::shelves::visible_to("$1"),
        search_join = parts.join,
        condition = parts.condition,
        order = parts.order,
    ))
    .bind(user.0.id)
    .bind(&expr)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let tagged: Vec<PublicHit> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, u.username AS owner, ps.name AS shelf_name, {owned} AS owned
         FROM books b
         JOIN (
             SELECT sb.book_id, MIN(s.id) AS shelf_id
             FROM shelf_books sb
             JOIN shelves s ON s.id = sb.shelf_id
             WHERE {visible} AND s.owner_id != $1
             GROUP BY sb.book_id
         ) pick ON pick.book_id = b.id
         JOIN shelves ps ON ps.id = pick.shelf_id
         JOIN users u ON u.id = ps.owner_id
         WHERE {TAGGED}
         ORDER BY LOWER(b.title)
         LIMIT {GROUP_LIMIT}",
        owned = owned_expr("$1"),
        visible = crate::shelves::visible_to("$1"),
    ))
    .bind(user.0.id)
    .bind(&pattern)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let mut public = public;
    for hit in tagged {
        if !public.iter().any(|h| h.book.id == hit.book.id) {
            public.push(hit);
        }
    }

    let shelves: Vec<PublicShelf> = sqlx::query_as(&format!(
        "SELECT {columns}
         FROM shelves s
         JOIN users u ON u.id = s.owner_id
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE {visible} AND s.owner_id != $1
           AND LOWER(s.name) LIKE '%' || LOWER($2) || '%'
         GROUP BY s.id, u.id
         ORDER BY LOWER(s.name)
         LIMIT {GROUP_LIMIT}",
        columns = crate::public::PUBLIC_SHELF_COLUMNS,
        visible = crate::shelves::visible_to("$1"),
    ))
    .bind(user.0.id)
    .bind(&raw)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    Ok(Json(SearchResult { mine, my_shelves, public, shelves, audiobooks }))
}
