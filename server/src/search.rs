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

/// The caller's own shelves whose name contains the query. Shelf names are
/// short; a simple substring match beats FTS here.
async fn own_shelves(state: &AppState, user_id: i64, raw: &str) -> Result<Vec<OwnShelf>, Response> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
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
    .bind(user_id)
    .bind(raw)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))
}

/// Shelves other users share with the caller whose name contains the query.
async fn shared_shelves(state: &AppState, user_id: i64, raw: &str) -> Result<Vec<PublicShelf>, Response> {
    sqlx::query_as(&format!(
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
    .bind(user_id)
    .bind(raw)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))
}

/// The caller's own books that match: by the full-text index first, then
/// those with a tag that contains the query, which the index does not hold.
async fn own_books(state: &AppState, user_id: i64, raw: &str) -> Result<Vec<Book>, Response> {
    let Some(expr) = search_expr(state.backend, raw) else { return Ok(Vec::new()) };
    let parts = search_parts(state.backend);
    // Both queries bind ($1 = caller, $2 = match expression or tag pattern):
    // the FTS condition from search_parts hardcodes $2.
    let mut mine: Vec<Book> = sqlx::query_as(&format!(
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
    .bind(user_id)
    .bind(&expr)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let tagged: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         {progress_joins}
         WHERE b.owner_id = $1 AND {TAGGED}
         ORDER BY LOWER(b.title)
         LIMIT {GROUP_LIMIT}",
        percent = progress::progress_percent(state.backend),
        progress_joins = progress::PROGRESS_JOINS,
    ))
    .bind(user_id)
    .bind(like_pattern(raw))
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    for book in tagged {
        if !mine.iter().any(|b| b.id == book.id) {
            mine.push(book);
        }
    }
    Ok(mine)
}

/// Books on shelves other users share with the caller that match, found the
/// same two ways.
async fn shared_books(state: &AppState, user_id: i64, raw: &str) -> Result<Vec<PublicHit>, Response> {
    let Some(expr) = search_expr(state.backend, raw) else { return Ok(Vec::new()) };
    let parts = search_parts(state.backend);
    let select = format!(
        "SELECT {BOOK_COLUMNS_B}, u.username AS owner, ps.name AS shelf_name, {owned} AS owned
         FROM books b
         {{search_join}}
         JOIN (
             SELECT sb.book_id, MIN(s.id) AS shelf_id
             FROM shelf_books sb
             JOIN shelves s ON s.id = sb.shelf_id
             WHERE {visible} AND s.owner_id != $1
             GROUP BY sb.book_id
         ) pick ON pick.book_id = b.id
         JOIN shelves ps ON ps.id = pick.shelf_id
         JOIN users u ON u.id = ps.owner_id",
        owned = owned_expr("$1"),
        visible = crate::shelves::visible_to("$1"),
    );
    let mut public: Vec<PublicHit> = sqlx::query_as(&format!(
        "{select} WHERE {condition} ORDER BY {order} LIMIT {GROUP_LIMIT}",
        select = select.replace("{search_join}", parts.join),
        condition = parts.condition,
        order = parts.order,
    ))
    .bind(user_id)
    .bind(&expr)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let tagged: Vec<PublicHit> = sqlx::query_as(&format!(
        "{select} WHERE {TAGGED} ORDER BY LOWER(b.title) LIMIT {GROUP_LIMIT}",
        select = select.replace("{search_join}", ""),
    ))
    .bind(user_id)
    .bind(like_pattern(raw))
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    for hit in tagged {
        if !public.iter().any(|h| h.book.id == hit.book.id) {
            public.push(hit);
        }
    }
    Ok(public)
}

/// `raw` as a LIKE pattern in which its own wildcards match themselves.
fn like_pattern(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<Params>,
) -> Result<Json<SearchResult>, Response> {
    let raw = params.q.as_deref().unwrap_or("").trim().to_string();
    let id = user.0.id;
    let searchable = search_expr(state.backend, &raw).is_some();
    Ok(Json(SearchResult {
        audiobooks: crate::audiobooks::search(&state, id, &raw).await?,
        my_shelves: own_shelves(&state, id, &raw).await?,
        mine: own_books(&state, id, &raw).await?,
        public: shared_books(&state, id, &raw).await?,
        shelves: if searchable { shared_shelves(&state, id, &raw).await? } else { Vec::new() },
    }))
}
