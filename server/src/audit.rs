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
    log_db(&state.db, actor, action, details).await
}

/// `log()` for code that has the database but no running server, such as
/// a command run from the shell.
pub async fn log_db(db: &sqlx::AnyPool, actor: Actor<'_>, action: &str, details: Value) {
    let result = sqlx::query("INSERT INTO activity_log (at, actor_id, actor_name, action, details) VALUES ($1, $2, $3, $4, $5)")
        .bind(now_ts())
        .bind(actor.map(|(id, _)| id))
        .bind(actor.map(|(_, name)| name))
        .bind(action)
        .bind(details.to_string())
        .execute(db)
        .await;
    if let Err(e) = result {
        tracing::warn!("audit log: could not write {action}: {e}");
    }
}

/// Once a day: drop what has run out, here and in openlibrary.rs and
/// ratelimit.rs. Two server processes doing it at the same time, as during
/// a rolling update, do no harm: every step is a plain delete.
pub async fn prune_daily(state: AppState) {
    loop {
        prune(&state).await;
        tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
    }
}

/// Log entries older than a year, sessions past their expiry, and
/// self-registrations never confirmed, whose names become free again. An
/// invitation is not touched: it stays listed until an admin withdraws or
/// renews it.
pub(crate) async fn prune(state: &AppState) {
    let year_ago = (time::OffsetDateTime::now_utc() - time::Duration::days(365))
        .format(time::macros::format_description!("[year]-[month]-[day]T00:00:00.000Z"))
        .unwrap_or_default();
    let now = now_ts();
    let steps = [
        ("log entries", "DELETE FROM activity_log WHERE at < $1", year_ago),
        ("sessions", "DELETE FROM sessions WHERE expires_at < $1", now.clone()),
        ("unconfirmed registrations", "DELETE FROM users WHERE verify_token IS NOT NULL AND verify_expires < $1", now),
    ];
    for (what, sql, cutoff) in steps {
        match sqlx::query(sql).bind(cutoff).execute(&state.db).await {
            Ok(done) if done.rows_affected() > 0 => tracing::info!("pruned {} expired {what}", done.rows_affected()),
            Ok(_) => {}
            Err(e) => tracing::warn!("pruning {what} failed: {e}"),
        }
    }
    crate::openlibrary::prune(state).await;
    crate::ratelimit::prune(state).await;
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

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, test_app};

    #[tokio::test]
    async fn pruning_drops_expired_sessions_and_unconfirmed_registrations() {
        let (_, db, state) = test_app().await;
        let (ada, _) = add_user(&db, "ada").await;
        let past = "2000-01-01T00:00:00.000Z";
        let future = "2999-01-01T00:00:00.000Z";
        sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ('old', ?, ?)").bind(ada).bind(past).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO users (username, password_hash, verify_token, verify_expires) VALUES ('pending', 'x', 't1', ?)").bind(future).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO users (username, password_hash, verify_token, verify_expires) VALUES ('stale', 'x', 't2', ?)").bind(past).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO users (username, password_hash, invite_token, invite_expires) VALUES ('invited', 'x', 't3', ?)").bind(past).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO activity_log (at, action, details) VALUES (?, 'user.registered', '{}')").bind(past).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO activity_log (at, action, details) VALUES (?, 'user.registered', '{}')").bind(crate::db::now_ts()).execute(&db).await.unwrap();

        super::prune(&state).await;

        let sessions: Vec<String> = sqlx::query_scalar("SELECT token FROM sessions ORDER BY token").fetch_all(&db).await.unwrap();
        assert_eq!(sessions, ["session-ada"]);
        // The stale registration is gone; the pending one and the expired
        // invitation stay.
        let users: Vec<String> = sqlx::query_scalar("SELECT username FROM users ORDER BY username").fetch_all(&db).await.unwrap();
        assert_eq!(users, ["ada", "invited", "pending"]);
        let log: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log").fetch_one(&db).await.unwrap();
        assert_eq!(log, 1);
    }
}
