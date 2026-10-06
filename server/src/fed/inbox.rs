//! Incoming activities: POST /ap/inbox and /ap/shelves/{slug}/inbox.
//!
//! Order of checks: size (1 MB, by the router), signature coverage, Digest
//! and Date, domain allowed, rate limit, the actor's key, idempotency. Only
//! then is the activity itself looked at. Refusals are recorded with type,
//! actor and reason (never the body).

use super::remote::{self, id_of};
use super::sig;
use crate::db::now_ts;
use crate::AppState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use serde_json::{json, Value};

const RATE_PER_MINUTE: u32 = 60;

fn rate_ok(state: &AppState, domain: &str) -> bool {
    let mut map = state.fed.rate.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    let entry = map.entry(domain.to_string()).or_insert((now, 0));
    if now.duration_since(entry.0) > std::time::Duration::from_secs(60) {
        *entry = (now, 0);
    }
    entry.1 += 1;
    entry.1 <= RATE_PER_MINUTE
}

pub async fn post(State(state): State<AppState>, uri: Uri, headers: HeaderMap, body: Bytes) -> StatusCode {
    if super::active(&state).await.is_none() {
        return StatusCode::NOT_FOUND;
    }
    match handle(&state, &uri, &headers, &body).await {
        Ok(()) => StatusCode::ACCEPTED,
        Err((status, kind, actor, reason)) => {
            super::reject(&state, kind.as_deref(), actor.as_deref(), &reason).await;
            status
        }
    }
}

type Refusal = (StatusCode, Option<String>, Option<String>, String);

async fn handle(state: &AppState, uri: &Uri, headers: &HeaderMap, body: &[u8]) -> Result<(), Refusal> {
    let activity: Option<Value> = serde_json::from_slice(body).ok();
    let kind = activity.as_ref().and_then(|a| a.get("type")).and_then(Value::as_str).map(str::to_string);
    let actor = activity.as_ref().and_then(|a| a.get("actor")).and_then(id_of);
    let refuse = |status: StatusCode, reason: &str| (status, kind.clone(), actor.clone(), reason.to_string());

    // 1. The signature, before anything else is trusted.
    let raw = headers.get("signature").and_then(|v| v.to_str().ok()).ok_or_else(|| refuse(StatusCode::UNAUTHORIZED, sig::SigError::Missing.as_str()))?;
    let parsed = sig::parse_signature(raw).ok_or_else(|| refuse(StatusCode::UNAUTHORIZED, sig::SigError::Malformed.as_str()))?;
    sig::precheck(&parsed, headers, Some(body)).map_err(|e| refuse(StatusCode::UNAUTHORIZED, e.as_str()))?;
    let activity = activity.ok_or_else(|| refuse(StatusCode::BAD_REQUEST, "not JSON"))?;
    let actor = actor.clone().ok_or_else(|| refuse(StatusCode::BAD_REQUEST, "activity without actor"))?;
    let key_owner = sig::key_owner(&parsed.key_id).to_string();
    let domain = super::domain_of(&actor).ok_or_else(|| refuse(StatusCode::BAD_REQUEST, "invalid actor"))?;
    if super::domain_of(&key_owner).as_deref() != Some(domain.as_str()) {
        return Err(refuse(StatusCode::UNAUTHORIZED, "keyId and actor on different domains"));
    }

    // 2. Who may talk to us at all.
    if !super::domain_allowed(state, &domain).await {
        // Let the admin know that an undecided instance wants to follow.
        if kind.as_deref() == Some("Follow") && super::requests::undecided(state, &domain).await {
            let target = activity.get("object").and_then(id_of).unwrap_or_default();
            let shelf = super::active(state).await.and_then(|c| c.slug_of(&target)).unwrap_or(target);
            super::requests::record(state, &domain, "in", &shelf, &actor).await;
        }
        return Err(refuse(StatusCode::FORBIDDEN, "domain not allowed"));
    }
    if !rate_ok(state, &domain) {
        return Err(refuse(StatusCode::TOO_MANY_REQUESTS, "too many activities"));
    }

    // 3. The key. A cached key that fails is fetched again once (rotation).
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    let mut key_actor = match remote::cached_actor(state, &key_owner).await {
        Some(a) => a,
        // A server announces a deleted account to every server it knows. One
        // that was never seen here leaves nothing to remove, and its key is
        // already gone, so the activity is dropped without a fetch.
        None if kind.as_deref() == Some("Delete") && activity.get("object").and_then(id_of).as_deref() == Some(actor.as_str()) => {
            return Ok(());
        }
        None => match remote::fetch_actor(state, &key_owner).await {
            Ok(a) => a,
            // A deleted account signs its own Delete; there is no key left to check.
            Err(super::net::FetchError::Status(410)) if kind.as_deref() == Some("Delete") => return Ok(()),
            Err(e) => return Err(refuse(StatusCode::UNAUTHORIZED, &format!("could not fetch key: {e}"))),
        },
    };
    let verified = |a: &remote::RemoteActor| {
        a.public_pem.as_deref().is_some_and(|pem| sig::verify(&parsed, "POST", path, headers, pem).is_ok())
    };
    if !verified(&key_actor) {
        key_actor = remote::fetch_actor(state, &key_owner)
            .await
            .map_err(|e| refuse(StatusCode::UNAUTHORIZED, &format!("could not fetch key: {e}")))?;
        if !verified(&key_actor) {
            return Err(refuse(StatusCode::UNAUTHORIZED, sig::SigError::BadSignature.as_str()));
        }
    }
    if key_owner != actor {
        return Err(refuse(StatusCode::UNAUTHORIZED, "signed by an actor other than actor"));
    }

    // 4. Once only.
    if let Some(id) = activity.get("id").and_then(Value::as_str) {
        let fresh = sqlx::query("INSERT INTO ap_inbox_seen (id, seen_at) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .bind(now_ts())
            .execute(&state.db)
            .await
            .map(|r| r.rows_affected() > 0)
            .unwrap_or(true);
        if !fresh {
            return Ok(());
        }
    }
    let _ = sqlx::query("UPDATE ap_instances SET last_seen_at = $1 WHERE domain = $2")
        .bind(now_ts())
        .bind(&domain)
        .execute(&state.db)
        .await;

    dispatch(state, &actor, &key_actor, &activity).await.map_err(|reason| refuse(StatusCode::UNPROCESSABLE_ENTITY, &reason))
}

async fn dispatch(state: &AppState, actor: &str, remote_actor: &remote::RemoteActor, a: &Value) -> Result<(), String> {
    let c = super::active(state).await.ok_or("federation is off")?;
    let kind = a.get("type").and_then(Value::as_str).unwrap_or("");
    let object = a.get("object").cloned().unwrap_or(Value::Null);
    let object_type = object.get("type").and_then(Value::as_str).unwrap_or("");
    let db = |e: sqlx::Error| e.to_string();

    match kind {
        "Follow" => {
            let target = id_of(&object).ok_or("Follow without object")?;
            let slug = c.slug_of(&target).ok_or("Follow of something that is not a local shelf")?;
            let shelf: Option<i64> =
                sqlx::query_scalar("SELECT id FROM shelves WHERE lower(ap_slug) = lower($1) AND visibility = 'federated'")
                    .bind(&slug)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(db)?;
            let inbox = remote_actor.shared_inbox.clone().or(remote_actor.inbox.clone()).ok_or("actor has no inbox")?;
            let reply_to = remote_actor.inbox.clone().unwrap_or(inbox.clone());
            let shelf_actor = c.shelf_actor(&slug);
            let mut follow = a.clone();
            if let Some(o) = follow.as_object_mut() {
                o.remove("@context");
            }
            let Some(shelf_id) = shelf else {
                let reply = json!({ "@context": super::objects::context(), "id": super::activity_id(&c),
                    "type": "Reject", "actor": shelf_actor, "object": follow });
                super::deliver::enqueue(state, &[reply_to], &shelf_actor, &reply).await.map_err(|e| e.to_string())?;
                return Ok(());
            };
            sqlx::query(
                "INSERT INTO ap_followers (shelf_id, actor_iri, inbox, accepted_at) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (shelf_id, actor_iri) DO UPDATE SET inbox = excluded.inbox",
            )
            .bind(shelf_id)
            .bind(actor)
            .bind(&inbox)
            .bind(now_ts())
            .execute(&state.db)
            .await
            .map_err(db)?;
            let reply = json!({ "@context": super::objects::context(), "id": super::activity_id(&c),
                "type": "Accept", "actor": shelf_actor, "object": follow });
            super::deliver::enqueue(state, &[reply_to], &shelf_actor, &reply).await.map_err(|e| e.to_string())?;
            tracing::info!("fed: {actor} follows {slug}");
            crate::audit::log(state, None, "fed.followed", json!({ "actor": actor, "handle": format!("@{slug}@{}", c.host) })).await;
        }
        "Undo" if object_type == "Follow" || object.is_string() => {
            if object.get("actor").and_then(id_of).is_some_and(|x| x != actor) {
                return Err("Undo of another actor's Follow".into());
            }
            let target = object.get("object").and_then(id_of).unwrap_or_default();
            let slug = c.slug_of(&target).unwrap_or_default();
            sqlx::query(
                "DELETE FROM ap_followers WHERE actor_iri = $1
                 AND ($2 = '' OR shelf_id IN (SELECT id FROM shelves WHERE lower(ap_slug) = lower($2)))",
            )
            .bind(actor)
            .bind(&slug)
            .execute(&state.db)
            .await
            .map_err(db)?;
            tracing::info!("fed: {actor} unfollowed {slug}");
            crate::audit::log(state, None, "fed.unfollowed", json!({ "actor": actor, "handle": format!("@{slug}@{}", c.host) })).await;
        }
        "Accept" | "Reject" => {
            // Our Follow of `actor` (a remote shelf) was answered.
            let followed = object.get("object").and_then(id_of);
            if followed.is_some_and(|f| f != actor) {
                return Err("reply to a Follow of another actor".into());
            }
            let new_state = if kind == "Accept" { "accepted" } else { "rejected" };
            let changed = sqlx::query("UPDATE ap_follows SET state = $1 WHERE remote_actor_iri = $2 AND state <> 'gone'")
                .bind(new_state)
                .bind(actor)
                .execute(&state.db)
                .await
                .map_err(db)?;
            if changed.rows_affected() == 0 {
                return Err("reply to a Follow we did not send".into());
            }
            tracing::info!("fed: {actor} replied {kind}");
            if kind == "Accept" {
                tokio::spawn(remote::backfill(state.clone(), actor.to_string()));
            }
        }
        "Create" | "Update" if object_type == "Page" => {
            if !remote::followed(state, actor).await {
                return Err("book from a shelf nobody here follows".into());
            }
            let book = remote::parse_book(actor, &object)?;
            remote::store_book(state, actor, &book).await.map_err(|e| e.to_string())?;
            tracing::info!("fed: {kind} {} from {actor}", book.object_iri);
        }
        "Update" if object_type == "Service" => {
            remote::fetch_actor(state, actor).await.map_err(|e| e.to_string())?;
        }
        "Delete" => {
            let target = id_of(&object).ok_or("Delete without object")?;
            if target == actor {
                // The shelf is gone (or no longer federated).
                super::requests::mark_gone(state, actor).await.map_err(db)?;
            } else {
                sqlx::query("UPDATE ap_remote_books SET deleted_at = $1 WHERE object_iri = $2 AND shelf_actor_iri = $3")
                    .bind(now_ts())
                    .bind(&target)
                    .bind(actor)
                    .execute(&state.db)
                    .await
                    .map_err(db)?;
            }
        }
        // Likes, boosts, replies from Mastodon readers: nothing to do.
        _ => {}
    }
    Ok(())
}
