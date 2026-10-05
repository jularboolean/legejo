//! Audit log stored in `activity_log`: account, invitation, book, shelf,
//! admin and federation events.
//!
//! `log()` never fails the calling request; losing a log line is preferable
//! to failing an upload. Actions are fixed codes grouped by the prefix before
//! the dot (user, account, book, shelf, admin, fed), which the admin page
//! filters on. Details include names as well as ids so entries stay readable
//! after the referenced book or shelf is deleted.

use crate::auth::{AdminUser, UserInfo};
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who did it: a user (id + name at the time) or nobody (the system, a
/// remote server, an anonymous visitor).
pub type Actor<'a> = Option<(i64, &'a str)>;

pub fn by(user: &UserInfo) -> Actor<'_> {
    Some((user.id, user.username.as_str()))
}

pub async fn log(state: &AppState, actor: Actor<'_>, action: &str, details: Value) {
    let result = sqlx::query("INSERT INTO activity_log (at, actor_id, actor_name, action, details) VALUES ($1, $2, $3, $4, $5)")
        .bind(now_ts())
        .bind(actor.map(|(id, _)| id))
        .bind(actor.map(|(_, name)| name))
        .bind(action)
        .bind(details.to_string())
        .execute(&state.db)
        .await;
    if let Err(e) = result {
        tracing::warn!("audit log: could not write {action}: {e}");
    }
}

/// Drop entries older than a year, once a day.
pub async fn prune_daily(state: AppState) {
    loop {
        let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::days(365))
            .format(time::macros::format_description!("[year]-[month]-[day]T00:00:00.000Z"))
            .unwrap_or_default();
        if let Err(e) = sqlx::query("DELETE FROM activity_log WHERE at < $1").bind(cutoff).execute(&state.db).await {
            tracing::warn!("audit log: pruning failed: {e}");
        }
        crate::openlibrary::prune(&state).await;
        crate::ratelimit::prune(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
    }
}

#[derive(Deserialize)]
pub struct LogQuery {
    /// Entries older than this id (paging backwards).
    before: Option<i64>,
    limit: Option<i64>,
    /// Action group ("book") or exact action ("book.uploaded").
    action: Option<String>,
    /// Only this user's actions.
    user: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Entry {
    id: i64,
    at: String,
    actor_id: Option<i64>,
    actor_name: Option<String>,
    action: String,
    #[serde(skip)]
    details: String,
    #[sqlx(skip)]
    #[serde(rename = "details")]
    details_json: Value,
}

#[derive(Serialize)]
pub struct Page {
    entries: Vec<Entry>,
    /// Pass as `before` for the next page; null at the end.
    next: Option<i64>,
}

pub async fn list(State(state): State<AppState>, _admin: AdminUser, Query(q): Query<LogQuery>) -> Result<Json<Page>, Response> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let action = q.action.unwrap_or_default();
    // "book" matches book.*; "book.uploaded" matches exactly.
    let (exact, prefix) = if action.contains('.') { (action.clone(), String::new()) } else if action.is_empty() {
        (String::new(), String::new())
    } else {
        (String::new(), format!("{action}."))
    };
    let mut entries: Vec<Entry> = sqlx::query_as(
        "SELECT id, at, actor_id, actor_name, action, details FROM activity_log
         WHERE ($1 = 0 OR id < $1)
           AND ($2 = '' OR action = $2)
           AND ($3 = '' OR action LIKE $3 || '%')
           AND ($4 = 0 OR actor_id = $4)
         ORDER BY id DESC LIMIT $5",
    )
    .bind(q.before.unwrap_or(0))
    .bind(&exact)
    .bind(&prefix)
    .bind(q.user.unwrap_or(0))
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("internal error: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
    })?;
    let next = if entries.len() as i64 > limit {
        entries.truncate(limit as usize);
        entries.last().map(|e| e.id)
    } else {
        None
    };
    for e in &mut entries {
        e.details_json = serde_json::from_str(&e.details).unwrap_or(Value::Null);
    }
    Ok(Json(Page { entries, next }))
}
