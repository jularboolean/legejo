//! Admin endpoints for federation: mode, allowed and blocked instances, and
//! an overview of federated shelves, the delivery queue and rejections.

use super::{net, Mode};
use crate::auth::AdminUser;
use crate::db::now_ts;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

async fn set(state: &AppState, key: &str, value: &str) -> Result<(), Response> {
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    Ok(())
}

#[derive(Serialize)]
pub struct FedSettings {
    public_url: Option<String>,
    mode: Mode,
    contact: Option<String>,
    max_epub_mb: usize,
}

async fn current(state: &AppState) -> FedSettings {
    FedSettings {
        public_url: state.fed.config.as_ref().map(|c| c.base.clone()),
        mode: super::mode(state).await,
        contact: super::setting(state, "federation_contact").await.filter(|c| !c.is_empty()),
        max_epub_mb: super::max_epub_bytes(state).await / (1024 * 1024),
    }
}

pub async fn get_settings(State(state): State<AppState>, _admin: AdminUser) -> Json<FedSettings> {
    Json(current(&state).await)
}

#[derive(Deserialize)]
pub struct UpdateSettings {
    mode: Mode,
    contact: Option<String>,
    max_epub_mb: Option<usize>,
}

pub async fn update_settings(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<UpdateSettings>,
) -> Result<Json<FedSettings>, Response> {
    if req.mode != Mode::Off && state.fed.config.is_none() {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "no public url"));
    }
    let before = super::mode(&state).await;
    set(&state, "federation_mode", req.mode.as_str()).await?;
    set(&state, "federation_contact", req.contact.as_deref().unwrap_or("").trim()).await?;
    if let Some(mb) = req.max_epub_mb {
        set(&state, "federation_max_epub_mb", &mb.clamp(1, 200).to_string()).await?;
    }
    if before != req.mode {
        tracing::info!("fed: mode changed {} -> {} by {}", before.as_str(), req.mode.as_str(), admin.0.username);
        crate::audit::log(&state, crate::audit::by(&admin.0), "fed.mode", json!({ "from": before.as_str(), "to": req.mode.as_str() })).await;
    }
    state.fed.wake.notify_one();
    Ok(Json(current(&state).await))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Instance {
    domain: String,
    status: String,
    software: Option<String>,
    note: Option<String>,
    added_at: String,
    last_seen_at: Option<String>,
}

pub async fn list_instances(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<Vec<Instance>>, Response> {
    Ok(Json(
        sqlx::query_as("SELECT domain, status, software, note, added_at, last_seen_at FROM ap_instances ORDER BY domain")
            .fetch_all(&state.db)
            .await
            .map_err(internal)?,
    ))
}

#[derive(Deserialize)]
pub struct DomainRequest {
    domain: String,
}

fn clean_domain(d: &str) -> Option<String> {
    let d = d.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_ascii_lowercase();
    let ok = !d.is_empty()
        && d.len() <= 253
        && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':');
    ok.then_some(d)
}

#[derive(Serialize)]
pub struct Preview {
    domain: String,
    software: Option<String>,
    version: Option<String>,
    contact: Option<String>,
    federation_mode: Option<String>,
}

async fn fetch_nodeinfo(state: &AppState, domain: &str) -> Result<Preview, &'static str> {
    let config = state.fed.config.as_ref().ok_or("no public url")?;
    let scheme = if config.allow_private && (domain.starts_with("127.") || domain.starts_with("localhost")) { "http" } else { "https" };
    let links = net::get_plain_json(state, &format!("{scheme}://{domain}/.well-known/nodeinfo"))
        .await
        .map_err(|_| "no nodeinfo")?;
    let href = links
        .get("links")
        .and_then(Value::as_array)
        .and_then(|l| l.iter().filter_map(|x| x.get("href").and_then(Value::as_str)).last())
        .ok_or("no nodeinfo")?
        .to_string();
    if super::domain_of(&href).as_deref() != Some(domain) {
        return Err("nodeinfo on another host");
    }
    let info = net::get_plain_json(state, &href).await.map_err(|_| "no nodeinfo")?;
    let s = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    Ok(Preview {
        domain: domain.to_string(),
        software: s(info.pointer("/software/name")),
        version: s(info.pointer("/software/version")),
        contact: s(info.pointer("/metadata/contact")),
        federation_mode: s(info.pointer("/metadata/federationMode")),
    })
}

pub async fn preview(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<DomainRequest>,
) -> Result<Json<Preview>, Response> {
    let domain = clean_domain(&req.domain).ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "invalid domain"))?;
    if super::active(&state).await.is_none() {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "federation is off"));
    }
    fetch_nodeinfo(&state, &domain).await.map(Json).map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))
}

#[derive(Deserialize)]
pub struct SetInstance {
    status: String,
    note: Option<String>,
}

pub async fn set_instance(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(domain): Path<String>,
    Json(req): Json<SetInstance>,
) -> Result<Json<Instance>, Response> {
    let domain = clean_domain(&domain).ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "invalid domain"))?;
    if req.status != "allowed" && req.status != "blocked" {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "invalid status"));
    }
    let software = if req.status == "allowed" && super::active(&state).await.is_some() {
        fetch_nodeinfo(&state, &domain).await.ok().and_then(|p| match (p.software, p.version) {
            (Some(s), Some(v)) => Some(format!("{s} {v}")),
            (s, _) => s,
        })
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO ap_instances (domain, status, software, note, added_by, added_at) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (domain) DO UPDATE SET status = excluded.status,
             software = COALESCE(excluded.software, ap_instances.software), note = excluded.note",
    )
    .bind(&domain)
    .bind(&req.status)
    .bind(&software)
    .bind(req.note.as_deref().map(str::trim).filter(|n| !n.is_empty()))
    .bind(admin.0.id)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(internal)?;
    tracing::info!("fed: {domain} -> {} by {}", req.status, admin.0.username);
    crate::audit::log(&state, crate::audit::by(&admin.0), "fed.instance", json!({ "domain": domain, "status": req.status })).await;
    if req.status == "blocked" {
        super::requests::on_refused(&state, &domain).await.map_err(internal)?;
        cut_off(&state, &domain).await.map_err(internal)?;
    } else {
        super::requests::on_allowed(&state, &domain).await.map_err(internal)?;
    }
    let inst: Instance =
        sqlx::query_as("SELECT domain, status, software, note, added_at, last_seen_at FROM ap_instances WHERE domain = $1")
            .bind(&domain)
            .fetch_one(&state.db)
            .await
            .map_err(internal)?;
    Ok(Json(inst))
}

/// A block takes effect at once: their followers are removed, our users'
/// follows of their shelves end (the Undo is queued before the block would
/// prevent it), and cached actors and books are dropped. Books already
/// imported into the library stay.
async fn cut_off(state: &AppState, domain: &str) -> anyhow::Result<()> {
    let on_domain = |iri: &String| super::domain_of(iri).as_deref() == Some(domain);
    let followers: Vec<(i64, String)> = sqlx::query_as("SELECT shelf_id, actor_iri FROM ap_followers").fetch_all(&state.db).await?;
    for (shelf_id, actor) in followers.iter().filter(|(_, a)| on_domain(a)) {
        sqlx::query("DELETE FROM ap_followers WHERE shelf_id = $1 AND actor_iri = $2")
            .bind(shelf_id)
            .bind(actor)
            .execute(&state.db)
            .await?;
    }
    let follows: Vec<String> = sqlx::query_scalar("SELECT DISTINCT remote_actor_iri FROM ap_follows").fetch_all(&state.db).await?;
    for actor in follows.iter().filter(|a| on_domain(a)) {
        sqlx::query("DELETE FROM ap_follows WHERE remote_actor_iri = $1").bind(actor).execute(&state.db).await?;
        super::remote::end_follow_if_unused(state, actor).await?;
    }
    let books: Vec<String> = sqlx::query_scalar("SELECT object_iri FROM ap_remote_books").fetch_all(&state.db).await?;
    for iri in books.iter().filter(|i| on_domain(i)) {
        sqlx::query("DELETE FROM ap_remote_books WHERE object_iri = $1").bind(iri).execute(&state.db).await?;
    }
    sqlx::query("DELETE FROM ap_remote_actors WHERE domain = $1").bind(domain).execute(&state.db).await?;
    Ok(())
}

pub async fn delete_instance(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(domain): Path<String>,
) -> Result<StatusCode, Response> {
    let domain = clean_domain(&domain).ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "invalid domain"))?;
    let r = sqlx::query("DELETE FROM ap_instances WHERE domain = $1").bind(&domain).execute(&state.db).await.map_err(internal)?;
    if r.rows_affected() == 0 {
        return Err(err(StatusCode::NOT_FOUND, "not found"));
    }
    crate::audit::log(&state, crate::audit::by(&admin.0), "fed.instance", json!({ "domain": domain, "status": "removed" })).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ShelfRow {
    id: i64,
    name: String,
    owner: String,
    #[sqlx(skip)]
    handle: String,
    #[serde(skip)]
    ap_slug: String,
    followers: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Rejection {
    at: String,
    activity_type: Option<String>,
    actor: Option<String>,
    domain: Option<String>,
    reason: String,
}

pub async fn overview(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<Value>, Response> {
    let mut shelves: Vec<ShelfRow> = sqlx::query_as(
        "SELECT s.id, s.name, u.username AS owner, s.ap_slug,
                (SELECT COUNT(*) FROM ap_followers f WHERE f.shelf_id = s.id) AS followers
         FROM shelves s JOIN users u ON u.id = s.owner_id
         WHERE s.visibility = 'federated' AND s.ap_slug IS NOT NULL ORDER BY LOWER(s.name)",
    )
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let host = state.fed.config.as_ref().map(|c| c.host.clone()).unwrap_or_default();
    for s in &mut shelves {
        s.handle = format!("@{}@{host}", s.ap_slug);
    }
    let (length, oldest): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MIN(created_at) FROM ap_deliveries").fetch_one(&state.db).await.map_err(internal)?;
    let rejections: Vec<Rejection> = sqlx::query_as(
        "SELECT at, activity_type, actor, domain, reason FROM ap_rejections ORDER BY id DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let requests = super::requests::list(&state).await.map_err(internal)?;
    Ok(Json(json!({
        "requests": requests,
        "shelves": shelves,
        "queue": { "length": length, "oldest": oldest },
        "rejections": rejections,
    })))
}

/// DELETE /api/admin/federation/rejections: empty the list of refused activities.
pub async fn clear_rejections(State(state): State<AppState>, _admin: AdminUser) -> Result<StatusCode, Response> {
    sqlx::query("DELETE FROM ap_rejections").execute(&state.db).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
