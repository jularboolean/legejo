//! The outgoing queue (ap_deliveries) and the background worker.
//!
//! Activities are queued in the database and signed when sent, so a restart
//! loses nothing. Backoff 1 min, 5 min, 30 min, 2 h, 6 h, 12 h, then
//! the delivery is dropped (about 21 h of trying). The worker also runs the
//! reconciler and prunes the idempotency table.

use crate::db::now_ts;
use crate::AppState;
use serde_json::Value;

const BACKOFF_SECS: [i64; 6] = [60, 300, 1800, 7200, 21600, 43200];

fn ts_in_secs(secs: i64) -> String {
    let t = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
    let f = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");
    t.format(&f).unwrap_or_default()
}

/// Queue one activity for each distinct inbox.
pub async fn enqueue(state: &AppState, inboxes: &[String], actor_iri: &str, activity: &Value) -> anyhow::Result<()> {
    let body = activity.to_string();
    let mut seen = std::collections::HashSet::new();
    for inbox in inboxes.iter().filter(|i| seen.insert(i.as_str())) {
        sqlx::query(
            "INSERT INTO ap_deliveries (inbox, actor_iri, body, next_attempt_at, created_at) VALUES ($1, $2, $3, $4, $4)",
        )
        .bind(inbox)
        .bind(actor_iri)
        .bind(&body)
        .bind(now_ts())
        .execute(&state.db)
        .await?;
    }
    state.fed.wake.notify_one();
    Ok(())
}

/// The inboxes of everyone following a shelf (sharedInbox when known).
pub async fn follower_inboxes(state: &AppState, shelf_id: i64) -> anyhow::Result<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT DISTINCT inbox FROM ap_followers WHERE shelf_id = $1")
        .bind(shelf_id)
        .fetch_all(&state.db)
        .await?)
}

/// Send what is due. Returns how many were attempted.
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    let due: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, inbox, actor_iri, body, attempts FROM ap_deliveries
         WHERE next_attempt_at <= $1 ORDER BY next_attempt_at, id LIMIT 20",
    )
    .bind(now_ts())
    .fetch_all(&state.db)
    .await?;
    let n = due.len();
    for (id, inbox, actor, body, attempts) in due {
        let result = super::net::post_activity(state, &inbox, &actor, &body).await;
        let error = match result {
            Ok(code) if (200..300).contains(&code) => None,
            // Permanent refusals: retrying won't help. 404 is deliberately not
            // included, since a reverse proxy commonly answers 404 while the
            // instance behind it is down or redeploying.
            Ok(code) if matches!(code, 400 | 401 | 403 | 405 | 410 | 422) => {
                tracing::warn!("fed: delivery to {inbox} dropped: HTTP {code}");
                None
            }
            Ok(code) => Some(format!("HTTP {code}")),
            Err(e) => Some(e.to_string()),
        };
        match error {
            None => {
                sqlx::query("DELETE FROM ap_deliveries WHERE id = $1").bind(id).execute(&state.db).await?;
            }
            Some(e) if attempts as usize >= BACKOFF_SECS.len() => {
                tracing::warn!("fed: delivery to {inbox} dropped after {} attempts: {e}", attempts + 1);
                sqlx::query("DELETE FROM ap_deliveries WHERE id = $1").bind(id).execute(&state.db).await?;
            }
            Some(e) => {
                tracing::info!("fed: delivery to {inbox} failed ({e}), will retry");
                sqlx::query(
                    "UPDATE ap_deliveries SET attempts = attempts + 1, next_attempt_at = $1, last_error = $2 WHERE id = $3",
                )
                .bind(ts_in_secs(BACKOFF_SECS[attempts as usize]))
                .bind(&e)
                .bind(id)
                .execute(&state.db)
                .await?;
            }
        }
    }
    Ok(n)
}

/// The background loop: deliveries every few seconds or when woken, the
/// reconciler on every round, pruning hourly. Does nothing while
/// federation is off.
pub async fn worker(state: AppState) {
    let mut last_prune = std::time::Instant::now() - std::time::Duration::from_secs(3600);
    loop {
        tokio::select! {
            _ = state.fed.wake.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {}
        }
        if super::active(&state).await.is_none() {
            continue;
        }
        if let Err(e) = super::reconcile::run(&state).await {
            tracing::error!("fed: reconcile failed: {e:#}");
        }
        loop {
            match run_once(&state).await {
                Ok(20) => continue,
                Ok(_) => break,
                Err(e) => {
                    tracing::error!("fed: delivery queue: {e:#}");
                    break;
                }
            }
        }
        if last_prune.elapsed() > std::time::Duration::from_secs(3600) {
            last_prune = std::time::Instant::now();
            let week_ago = ts_in_secs(-7 * 24 * 3600);
            let _ = sqlx::query("DELETE FROM ap_inbox_seen WHERE seen_at < $1").bind(week_ago).execute(&state.db).await;
            super::requests::upkeep(&state).await;
        }
    }
}
