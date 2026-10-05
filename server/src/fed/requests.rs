//! Instances waiting for an admin decision, and the upkeep of follows.
//!
//! In allowlist mode an unknown instance is refused. So that the admin hears
//! about it, both directions leave a request: a remote instance that tried to
//! follow a shelf here, and a local user who wants to follow a shelf there.
//! Allowing the instance sends the follows that were held back; the remote
//! side retries its own Follow (see `resend_pending`).
//!
//! The same hourly pass checks that followed shelves still answer. One that
//! does not is shown as unreachable at once and treated as gone after a week.

use super::net::FetchError;
use super::remote;
use crate::auth::AdminUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

/// Upper bound on stored requests, so refused traffic cannot grow the table.
const MAX_REQUESTS: i64 = 200;
/// A pending Follow is sent again this often, for this long.
const RESEND_AFTER_HOURS: i64 = 6;
const RESEND_FOR_DAYS: i64 = 14;
/// Followed shelves are checked this often; unreachable for this long means gone.
const CHECK_EVERY_HOURS: i64 = 6;
const GONE_AFTER_DAYS: i64 = 7;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

/// Is this domain neither allowed nor blocked while the instance runs an
/// allowlist, i.e. a case for the admin to decide?
pub async fn undecided(state: &AppState, domain: &str) -> bool {
    if super::mode(state).await != super::Mode::Allowlist {
        return false;
    }
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM ap_instances WHERE domain = $1")
        .bind(domain.to_ascii_lowercase())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    status.is_none()
}

/// Record a request, or count another attempt of one already recorded.
pub async fn record(state: &AppState, domain: &str, direction: &str, detail: &str, requested_by: &str) {
    let domain = domain.to_ascii_lowercase();
    let known: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_instance_requests WHERE domain = $1")
        .bind(&domain)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_instance_requests").fetch_one(&state.db).await.unwrap_or(0);
    if known == 0 && total >= MAX_REQUESTS {
        return;
    }
    let now = now_ts();
    let result = sqlx::query(
        "INSERT INTO ap_instance_requests (domain, direction, detail, requested_by, first_at, last_at)
         VALUES ($1, $2, $3, $4, $5, $5)
         ON CONFLICT (domain, direction, detail, requested_by)
         DO UPDATE SET attempts = ap_instance_requests.attempts + 1, last_at = excluded.last_at",
    )
    .bind(&domain)
    .bind(direction)
    .bind(detail)
    .bind(requested_by)
    .bind(&now)
    .execute(&state.db)
    .await;
    if let Err(e) = result {
        tracing::warn!("fed: could not record a request from {domain}: {e}");
        return;
    }
    // One log entry per instance, when it first shows up.
    if known == 0 {
        tracing::info!("fed: {domain} awaits an admin decision ({direction})");
        crate::audit::log(state, None, "fed.instance_requested", json!({ "domain": domain, "direction": direction })).await;
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Request {
    domain: String,
    direction: String,
    detail: String,
    requested_by: String,
    attempts: i64,
    first_at: String,
    last_at: String,
}

pub async fn list(state: &AppState) -> Result<Vec<Request>, sqlx::Error> {
    sqlx::query_as(
        "SELECT domain, direction, detail, requested_by, attempts, first_at, last_at
         FROM ap_instance_requests ORDER BY last_at DESC, domain",
    )
    .fetch_all(&state.db)
    .await
}

/// GET /api/admin/federation/pending: how many instances await a decision.
pub async fn pending(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<serde_json::Value>, Response> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT domain) FROM ap_instance_requests")
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    Ok(Json(json!({ "count": count })))
}

/// The remote shelves on `domain` that local users follow.
async fn followed_on(state: &AppState, domain: &str, only_unsent: bool) -> anyhow::Result<Vec<String>> {
    let sql = if only_unsent {
        "SELECT DISTINCT remote_actor_iri FROM ap_follows WHERE state = 'pending' AND sent_at IS NULL"
    } else {
        "SELECT DISTINCT remote_actor_iri FROM ap_follows"
    };
    let all: Vec<String> = sqlx::query_scalar(sql).fetch_all(&state.db).await?;
    Ok(all.into_iter().filter(|a| super::domain_of(a).as_deref() == Some(domain)).collect())
}

/// The instance was allowed: forget its requests and send the held follows.
pub async fn on_allowed(state: &AppState, domain: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM ap_instance_requests WHERE domain = $1").bind(domain).execute(&state.db).await?;
    for actor in followed_on(state, domain, true).await? {
        if let Err(e) = send_follow(state, &actor).await {
            tracing::warn!("fed: could not send the held Follow of {actor}: {e:#}");
        }
    }
    Ok(())
}

/// The instance was blocked or the request dismissed: the held follows end.
pub async fn on_refused(state: &AppState, domain: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM ap_instance_requests WHERE domain = $1").bind(domain).execute(&state.db).await?;
    for actor in followed_on(state, domain, true).await? {
        sqlx::query("UPDATE ap_follows SET state = 'rejected' WHERE remote_actor_iri = $1 AND sent_at IS NULL")
            .bind(&actor)
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

/// DELETE /api/admin/federation/requests/{domain}: dismiss without a decision.
pub async fn dismiss(State(state): State<AppState>, admin: AdminUser, Path(domain): Path<String>) -> Result<StatusCode, Response> {
    let domain = domain.trim().to_ascii_lowercase();
    on_refused(&state, &domain).await.map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&admin.0), "fed.instance", json!({ "domain": domain, "status": "dismissed" })).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Queue the Follow for a remote shelf and note when it was sent.
pub async fn send_follow(state: &AppState, actor: &str) -> anyhow::Result<()> {
    let c = super::active(state).await.ok_or_else(|| anyhow::anyhow!("federation is off"))?;
    let inbox = remote::actor(state, actor)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .inbox
        .ok_or_else(|| anyhow::anyhow!("actor has no inbox"))?;
    super::deliver::enqueue(state, &[inbox], &c.instance_actor(), &remote::follow_activity(&c, actor)).await?;
    sqlx::query("UPDATE ap_follows SET sent_at = $1 WHERE remote_actor_iri = $2")
        .bind(now_ts())
        .bind(actor)
        .execute(&state.db)
        .await?;
    tracing::info!("fed: Follow {actor}");
    Ok(())
}

/// A Follow refused by an allowlist on the other side is dropped there. Send
/// pending follows again now and then, so that the follow goes through once
/// their admin has allowed this instance.
async fn resend_pending(state: &AppState) -> anyhow::Result<()> {
    let due: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT remote_actor_iri FROM ap_follows
         WHERE state = 'pending' AND sent_at IS NOT NULL AND sent_at < $1 AND created_at > $2",
    )
    .bind(crate::db::ts_in_hours(-RESEND_AFTER_HOURS))
    .bind(crate::db::ts_in_days(-RESEND_FOR_DAYS))
    .fetch_all(&state.db)
    .await?;
    for actor in due {
        let Some(domain) = super::domain_of(&actor) else { continue };
        if super::domain_allowed(state, &domain).await {
            if let Err(e) = send_follow(state, &actor).await {
                tracing::info!("fed: could not resend the Follow of {actor}: {e:#}");
            }
        }
    }
    Ok(())
}

/// The shelf no longer exists: end the follows and hide its books.
pub async fn mark_gone(state: &AppState, actor: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE ap_follows SET state = 'gone' WHERE remote_actor_iri = $1").bind(actor).execute(&state.db).await?;
    sqlx::query("UPDATE ap_remote_books SET deleted_at = $1 WHERE shelf_actor_iri = $2 AND deleted_at IS NULL")
        .bind(now_ts())
        .bind(actor)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE ap_remote_actors SET gone = 1 WHERE iri = $1").bind(actor).execute(&state.db).await?;
    sqlx::query("DELETE FROM ap_followers WHERE actor_iri = $1").bind(actor).execute(&state.db).await?;
    tracing::info!("fed: {actor} is gone");
    Ok(())
}

/// Fetch the actor document of a followed shelf and record the outcome.
pub async fn check(state: &AppState, actor: &str) {
    let now = now_ts();
    match remote::fetch_actor(state, actor).await {
        Ok(_) => {
            let _ = sqlx::query("UPDATE ap_remote_actors SET checked_at = $1, unreachable_since = NULL WHERE iri = $2")
                .bind(&now)
                .bind(actor)
                .execute(&state.db)
                .await;
        }
        // The instance answers and says the shelf is deleted.
        Err(FetchError::Status(410)) => {
            if let Err(e) = mark_gone(state, actor).await {
                tracing::warn!("fed: could not mark {actor} as gone: {e}");
            }
        }
        // Federation is off here: nothing can be said about the other side.
        Err(FetchError::Off) => {}
        // No answer, or an error that may pass (a reverse proxy answers 404
        // or 502 while the instance behind it is down).
        Err(e) => {
            let _ = sqlx::query(
                "UPDATE ap_remote_actors SET checked_at = $1, unreachable_since = COALESCE(unreachable_since, $1) WHERE iri = $2",
            )
            .bind(&now)
            .bind(actor)
            .execute(&state.db)
            .await;
            let since: Option<String> = sqlx::query_scalar("SELECT unreachable_since FROM ap_remote_actors WHERE iri = $1")
                .bind(actor)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .flatten();
            tracing::info!("fed: {actor} does not answer ({e})");
            if since.is_some_and(|s| s < crate::db::ts_in_days(-GONE_AFTER_DAYS)) {
                if let Err(e) = mark_gone(state, actor).await {
                    tracing::warn!("fed: could not mark {actor} as gone: {e}");
                }
            }
        }
    }
}

/// Check one shelf if it was not checked within `max_age_minutes`.
pub async fn check_if_stale(state: &AppState, actor: &str, max_age_minutes: i64) {
    let checked: Option<String> = sqlx::query_scalar("SELECT checked_at FROM ap_remote_actors WHERE iri = $1")
        .bind(actor)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten();
    let cutoff = (time::OffsetDateTime::now_utc() - time::Duration::minutes(max_age_minutes))
        .format(&crate::db::TS_FORMAT)
        .unwrap_or_default();
    if checked.is_none_or(|c| c < cutoff) {
        check(state, actor).await;
    }
}

/// The hourly pass: resend pending follows and check followed shelves.
pub async fn upkeep(state: &AppState) {
    if let Err(e) = resend_pending(state).await {
        tracing::warn!("fed: resending follows failed: {e:#}");
    }
    let followed: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT remote_actor_iri FROM ap_follows WHERE state IN ('pending', 'accepted') AND sent_at IS NOT NULL",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    for actor in followed {
        check_if_stale(state, &actor, CHECK_EVERY_HOURS * 60).await;
    }
}
