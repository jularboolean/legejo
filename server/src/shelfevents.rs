//! What has happened on a shared shelf, for its owner: who follows it here
//! or from another instance, who took a copy of one of its books, and what
//! was downloaded through a private link or by another instance.
//!
//! Recording never fails the request it belongs to. Names are kept as they
//! were at the time, so an entry stays readable after the book or the user
//! is gone. Entries older than a year are dropped with the audit log.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A download repeated within this many hours through the same way to the
/// same book is one entry: an app fetching a file twice is not news.
const SAME_DOWNLOAD_HOURS: i64 = 1;

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A user here followed the shelf.
    Followed,
    Unfollowed,
    /// A user here took a copy of a book into their own library.
    Imported,
    /// A book was downloaded through a private link; `detail` is the link's name.
    LinkDownload,
    /// Someone on another instance followed the shelf; the actor is their handle.
    FedFollowed,
    FedUnfollowed,
    /// A book's file was fetched through the federation, by someone unknown.
    FedDownload,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Followed => "followed",
            Kind::Unfollowed => "unfollowed",
            Kind::Imported => "imported",
            Kind::LinkDownload => "link_download",
            Kind::FedFollowed => "fed_followed",
            Kind::FedUnfollowed => "fed_unfollowed",
            Kind::FedDownload => "fed_download",
        }
    }

    fn is_download(self) -> bool {
        matches!(self, Kind::LinkDownload | Kind::FedDownload)
    }
}

/// One thing that happened, as recorded.
#[derive(Clone, Copy)]
pub struct Event<'a> {
    pub kind: Kind,
    /// A user here (id, name), or only a name (a handle on another instance).
    pub actor_id: Option<i64>,
    pub actor_name: Option<&'a str>,
    pub book: Option<(i64, &'a str)>,
    pub detail: Option<&'a str>,
}

/// Record an event on a shelf.
pub async fn record(state: &AppState, shelf_id: i64, event: Event<'_>) {
    let now = now_ts();
    if event.kind.is_download() {
        let since = crate::db::ts_in_hours(-SAME_DOWNLOAD_HOURS);
        let again: Result<Option<i64>, _> = sqlx::query_scalar(
            "SELECT id FROM shelf_events
             WHERE shelf_id = $1 AND kind = $2 AND book_id = $3 AND COALESCE(detail, '') = $4 AND at > $5 LIMIT 1",
        )
        .bind(shelf_id)
        .bind(event.kind.as_str())
        .bind(event.book.map(|(id, _)| id))
        .bind(event.detail.unwrap_or(""))
        .bind(&since)
        .fetch_optional(&state.db)
        .await;
        if matches!(again, Ok(Some(_))) {
            return;
        }
    }
    let result = sqlx::query(
        "INSERT INTO shelf_events (shelf_id, at, kind, actor_id, actor_name, book_id, book_title, detail)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(shelf_id)
    .bind(&now)
    .bind(event.kind.as_str())
    .bind(event.actor_id)
    .bind(event.actor_name)
    .bind(event.book.map(|(id, _)| id))
    .bind(event.book.map(|(_, title)| title))
    .bind(event.detail)
    .execute(&state.db)
    .await;
    if let Err(e) = result {
        tracing::warn!("shelf events: could not record {} on shelf {shelf_id}: {e}", event.kind.as_str());
    }
}

/// Record an event on every shelf of the book's owner that holds the book
/// and is shared in a way `shelf_filter` allows (an SQL condition on `s`).
pub async fn record_on_shelves_of(state: &AppState, book_id: i64, shelf_filter: &str, event: Event<'_>) {
    let shelves: Vec<i64> = sqlx::query_scalar(&format!(
        "SELECT s.id FROM shelves s JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE sb.book_id = $1 AND s.owner_id = (SELECT owner_id FROM books WHERE id = $1) AND ({shelf_filter})"
    ))
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    for shelf_id in shelves {
        record(state, shelf_id, event).await;
    }
}

/// A handle such as "@ada@example.org" for an actor on another instance.
pub fn handle(preferred_username: Option<&str>, domain: &str, iri: &str) -> String {
    match preferred_username.map(str::trim).filter(|u| !u.is_empty()) {
        Some(user) => format!("@{user}@{domain}"),
        None => iri.to_string(),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// Entries older than this id (paging backwards).
    before: Option<i64>,
    limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Entry {
    id: i64,
    at: String,
    kind: String,
    actor_id: Option<i64>,
    actor_name: Option<String>,
    book_id: Option<i64>,
    book_title: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
pub struct Page {
    entries: Vec<Entry>,
    /// Pass as `before` for the next page; null at the end.
    next: Option<i64>,
}

/// GET /api/shelves/{id}/events: the owner's view of what has happened on
/// the shelf, newest first.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(shelf_id): Path<i64>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Page>, Response> {
    let fail = |status: StatusCode, msg: &str| (status, Json(json!({ "error": msg }))).into_response();
    let internal = |e: sqlx::Error| {
        tracing::error!("internal error: {e}");
        fail(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    };
    let owner: Option<i64> =
        sqlx::query_scalar("SELECT owner_id FROM shelves WHERE id = $1").bind(shelf_id).fetch_optional(&state.db).await.map_err(internal)?;
    if owner != Some(user.0.id) {
        return Err(fail(StatusCode::NOT_FOUND, "not found"));
    }
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let mut entries: Vec<Entry> = sqlx::query_as(
        "SELECT id, at, kind, actor_id, actor_name, book_id, book_title, detail FROM shelf_events
         WHERE shelf_id = $1 AND ($2 = 0 OR id < $2)
         ORDER BY id DESC LIMIT $3",
    )
    .bind(shelf_id)
    .bind(q.before.unwrap_or(0))
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let next = if entries.len() as i64 > limit {
        entries.truncate(limit as usize);
        entries.last().map(|e| e.id)
    } else {
        None
    };
    Ok(Json(Page { entries, next }))
}
