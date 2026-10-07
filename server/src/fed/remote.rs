//! The receiving side: remote actors, following remote shelves, the books
//! they announce, and importing a book into a user's library. The license
//! gate runs again on everything that arrives.

use super::net::{self, FetchError};
use super::objects;
use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::license::{self, LicenseFacts};
use crate::AppState;
use axum::extract::{Query, State};
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

/// A string, or the `id` of an embedded object.
pub fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("id").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct RemoteActor {
    pub iri: String,
    pub domain: String,
    pub kind: Option<String>,
    pub preferred_username: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub inbox: Option<String>,
    pub shared_inbox: Option<String>,
    pub outbox: Option<String>,
    pub public_pem: Option<String>,
}

const ACTOR_COLUMNS: &str =
    "iri, domain, kind, preferred_username, name, summary, inbox, shared_inbox, outbox, public_pem";

pub async fn cached_actor(state: &AppState, iri: &str) -> Option<RemoteActor> {
    sqlx::query_as(&format!("SELECT {ACTOR_COLUMNS} FROM ap_remote_actors WHERE iri = $1 AND gone = 0"))
        .bind(iri)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

/// Fetch an actor document and cache it. The document must live where its
/// id says (no actor may speak for another host).
pub async fn fetch_actor(state: &AppState, iri: &str) -> Result<RemoteActor, FetchError> {
    let doc = net::get_json(state, iri).await?;
    let id = doc.get("id").and_then(Value::as_str).unwrap_or("");
    if id != iri || super::domain_of(id).is_none() {
        return Err(FetchError::Blocked(format!("actor id {id} does not match {iri}")));
    }
    let s = |k: &str| doc.get(k).and_then(Value::as_str).map(str::to_string);
    let key = doc.get("publicKey");
    let key_owner = key.and_then(|k| k.get("owner")).and_then(Value::as_str);
    let pem = if key_owner.is_none() || key_owner == Some(iri) {
        key.and_then(|k| k.get("publicKeyPem")).and_then(Value::as_str).map(str::to_string)
    } else {
        None
    };
    let actor = RemoteActor {
        iri: iri.to_string(),
        domain: super::domain_of(iri).unwrap_or_default(),
        kind: s("type"),
        preferred_username: s("preferredUsername"),
        name: s("name"),
        summary: s("summary"),
        inbox: s("inbox"),
        shared_inbox: doc.get("endpoints").and_then(|e| e.get("sharedInbox")).and_then(Value::as_str).map(str::to_string),
        outbox: s("outbox"),
        public_pem: pem,
    };
    sqlx::query(
        "INSERT INTO ap_remote_actors (iri, domain, kind, preferred_username, name, summary, inbox, shared_inbox, outbox, public_pem, fetched_at, gone)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0)
         ON CONFLICT (iri) DO UPDATE SET domain = excluded.domain, kind = excluded.kind,
             preferred_username = excluded.preferred_username, name = excluded.name, summary = excluded.summary,
             inbox = excluded.inbox, shared_inbox = excluded.shared_inbox, outbox = excluded.outbox,
             public_pem = excluded.public_pem, fetched_at = excluded.fetched_at, gone = 0",
    )
    .bind(&actor.iri)
    .bind(&actor.domain)
    .bind(&actor.kind)
    .bind(&actor.preferred_username)
    .bind(&actor.name)
    .bind(&actor.summary)
    .bind(&actor.inbox)
    .bind(&actor.shared_inbox)
    .bind(&actor.outbox)
    .bind(&actor.public_pem)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(|e| FetchError::Network(e.to_string()))?;
    Ok(actor)
}

/// The cached actor, or a fresh fetch.
pub async fn actor(state: &AppState, iri: &str) -> Result<RemoteActor, FetchError> {
    match cached_actor(state, iri).await {
        Some(a) => Ok(a),
        None => fetch_actor(state, iri).await,
    }
}

/// A remote book after validation: everything the gate and the download need.
#[derive(Debug, Clone)]
pub struct IncomingBook {
    pub object_iri: String,
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub summary: Option<String>,
    pub license: String,
    pub license_source_url: String,
    pub author_death_year: Option<i64>,
    pub epub_url: String,
    pub epub_sha256: String,
    pub epub_size: i64,
    pub cover_url: Option<String>,
    pub published: Option<String>,
}

/// Validate a Page object from `shelf_actor`. Err(reason) when refused.
pub fn parse_book(shelf_actor: &str, object: &Value) -> Result<IncomingBook, String> {
    let s = |k: &str| object.get(k).and_then(Value::as_str).map(str::to_string);
    let object_iri = s("id").ok_or("object without id")?;
    let actor_domain = super::domain_of(shelf_actor);
    if super::domain_of(&object_iri) != actor_domain {
        return Err("object is on a different host than the actor".into());
    }
    if object.get("attributedTo").and_then(id_of).as_deref() != Some(shelf_actor) {
        return Err("attributedTo is not the sender".into());
    }
    let facts = LicenseFacts {
        license: s("license"),
        source_url: s("licenseSource"),
        author_death_year: object.get("authorDeathYear").and_then(Value::as_i64),
    };
    if let Err(reason) = license::federable(&facts, license::current_year()) {
        return Err(format!("license: {}", serde_json::to_string(&reason).unwrap_or_default()));
    }
    let links = object.get("url").and_then(Value::as_array).cloned().unwrap_or_default();
    let epub = links
        .iter()
        .find(|l| l.get("mediaType").and_then(Value::as_str) == Some("application/epub+zip"))
        .ok_or("no EPUB link")?;
    let epub_url = epub.get("href").and_then(Value::as_str).ok_or("EPUB link without href")?.to_string();
    if super::domain_of(&epub_url) != actor_domain {
        return Err("EPUB file is on a different host than the actor".into());
    }
    let epub_sha256 = epub.get("sha256").and_then(Value::as_str).ok_or("EPUB without sha256")?.to_ascii_lowercase();
    if epub_sha256.len() != 64 || !epub_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid sha256".into());
    }
    let epub_size = epub.get("size").and_then(Value::as_i64).ok_or("EPUB without size")?;
    let cover_url = object.get("icon").and_then(|i| i.get("url")).and_then(id_of).filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    let title = s("name").filter(|t| !t.trim().is_empty()).ok_or("book without title")?;
    Ok(IncomingBook {
        object_iri,
        title: title.chars().take(500).collect(),
        author: s("author").map(|a| a.chars().take(500).collect()),
        language: s("language").map(|a| a.chars().take(50).collect()),
        isbn: s("isbn").map(|a| a.chars().take(20).collect()),
        summary: s("summary").map(|a| a.chars().take(2000).collect()),
        license: facts.license.unwrap_or_default(),
        license_source_url: facts.source_url.unwrap_or_default(),
        author_death_year: facts.author_death_year,
        epub_url,
        epub_sha256,
        epub_size,
        cover_url,
        published: s("published"),
    })
}

/// Store (or refresh) a received book.
pub async fn store_book(state: &AppState, shelf_actor: &str, b: &IncomingBook) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO ap_remote_books (object_iri, shelf_actor_iri, title, author, language, isbn, summary, license,
             license_source_url, author_death_year, epub_url, epub_sha256, epub_size, cover_url, published, received_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, CAST($10 AS BIGINT), $11, $12, $13, $14, $15, $16)
         ON CONFLICT (object_iri) DO UPDATE SET title = excluded.title, author = excluded.author,
             language = excluded.language, isbn = excluded.isbn, summary = excluded.summary, license = excluded.license,
             license_source_url = excluded.license_source_url, author_death_year = excluded.author_death_year,
             epub_url = excluded.epub_url, epub_sha256 = excluded.epub_sha256, epub_size = excluded.epub_size,
             cover_url = excluded.cover_url, deleted_at = NULL
         WHERE ap_remote_books.shelf_actor_iri = excluded.shelf_actor_iri",
    )
    .bind(&b.object_iri)
    .bind(shelf_actor)
    .bind(&b.title)
    .bind(&b.author)
    .bind(&b.language)
    .bind(&b.isbn)
    .bind(&b.summary)
    .bind(&b.license)
    .bind(&b.license_source_url)
    .bind(b.author_death_year.map(|y| y.to_string()))
    .bind(&b.epub_url)
    .bind(&b.epub_sha256)
    .bind(b.epub_size)
    .bind(&b.cover_url)
    .bind(&b.published)
    .bind(now_ts())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Do any of our users follow this shelf (accepted)?
pub async fn followed(state: &AppState, shelf_actor: &str) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ap_follows WHERE remote_actor_iri = $1 AND state = 'accepted'")
        .bind(shelf_actor)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
        > 0
}

/// After an Accept: read the shelf's outbox and ingest what is already
/// there. At most 50 collection pages are followed.
pub async fn backfill(state: AppState, shelf_actor: String) {
    let Ok(actor) = actor(&state, &shelf_actor).await else { return };
    let Some(outbox) = actor.outbox else { return };
    let mut next = match net::get_json(&state, &outbox).await {
        Ok(collection) => {
            if let Some(items) = collection.get("orderedItems").and_then(Value::as_array) {
                ingest_items(&state, &shelf_actor, items).await;
                None
            } else {
                collection.get("first").and_then(id_of)
            }
        }
        Err(e) => {
            tracing::warn!("fed: outbox {outbox}: {e}");
            None
        }
    };
    let mut pages = 0;
    while let Some(url) = next.take() {
        pages += 1;
        if pages > 50 || super::domain_of(&url) != super::domain_of(&shelf_actor) {
            break;
        }
        match net::get_json(&state, &url).await {
            Ok(page) => {
                if let Some(items) = page.get("orderedItems").and_then(Value::as_array) {
                    ingest_items(&state, &shelf_actor, items).await;
                }
                next = page.get("next").and_then(id_of);
            }
            Err(e) => tracing::warn!("fed: outbox page {url}: {e}"),
        }
    }
}

async fn ingest_items(state: &AppState, shelf_actor: &str, items: &[Value]) {
    for item in items {
        if item.get("type").and_then(Value::as_str) != Some("Create") {
            continue;
        }
        let Some(object) = item.get("object") else { continue };
        match parse_book(shelf_actor, object) {
            Ok(book) => {
                if let Err(e) = store_book(state, shelf_actor, &book).await {
                    tracing::error!("fed: could not store {}: {e:#}", book.object_iri);
                }
            }
            Err(reason) => {
                super::reject(state, Some("Create"), Some(shelf_actor), &format!("outbox: {reason}")).await;
            }
        }
    }
}

// ---------------------------------------------------------------- user API

#[derive(Serialize)]
pub struct FedStatus {
    available: bool,
    mode: super::Mode,
    host: Option<String>,
    /// Instances awaiting an admin decision; only reported to admins.
    pending_instances: i64,
}

pub async fn status(State(state): State<AppState>, user: AuthUser) -> Json<FedStatus> {
    let mode = super::mode(&state).await;
    let host = state.fed.config.as_ref().map(|c| c.host.clone());
    let pending_instances = if user.0.is_admin && mode == super::Mode::Allowlist {
        sqlx::query_scalar("SELECT COUNT(DISTINCT domain) FROM ap_instance_requests").fetch_one(&state.db).await.unwrap_or(0)
    } else {
        0
    };
    Json(FedStatus { available: host.is_some() && mode != super::Mode::Off, mode, host, pending_instances })
}

#[derive(Serialize, sqlx::FromRow)]
pub struct FollowItem {
    actor: String,
    #[sqlx(skip)]
    handle: String,
    name: Option<String>,
    summary: Option<String>,
    state: String,
    book_count: i64,
    /// The Follow is held back until an admin allows the instance.
    #[sqlx(skip)]
    awaiting_approval: bool,
    /// Set while the shelf's instance does not answer.
    unreachable_since: Option<String>,
    #[serde(skip)]
    sent_at: Option<String>,
    #[serde(skip)]
    preferred_username: Option<String>,
    #[serde(skip)]
    domain: Option<String>,
}

async fn follow_items(state: &AppState, user_id: i64, only: Option<&str>) -> Result<Vec<FollowItem>, Response> {
    let mut items: Vec<FollowItem> = sqlx::query_as(
        "SELECT f.remote_actor_iri AS actor, a.name, a.summary, f.state, a.preferred_username, a.domain,
                f.sent_at, a.unreachable_since,
                (SELECT COUNT(*) FROM ap_remote_books r WHERE r.shelf_actor_iri = f.remote_actor_iri AND r.deleted_at IS NULL) AS book_count
         FROM ap_follows f LEFT JOIN ap_remote_actors a ON a.iri = f.remote_actor_iri
         WHERE f.user_id = $1 AND ($2 = '' OR f.remote_actor_iri = $2)
         ORDER BY LOWER(COALESCE(a.name, f.remote_actor_iri))",
    )
    .bind(user_id)
    .bind(only.unwrap_or(""))
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    for item in &mut items {
        item.awaiting_approval = item.state == "pending" && item.sent_at.is_none();
        item.handle = match (&item.preferred_username, &item.domain) {
            (Some(u), Some(d)) => format!("@{u}@{d}"),
            _ => item.actor.clone(),
        };
    }
    Ok(items)
}

pub async fn list_follows(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<FollowItem>>, Response> {
    Ok(Json(follow_items(&state, user.0.id, None).await?))
}

#[derive(Deserialize)]
pub struct FollowRequest {
    handle: String,
}

/// Resolve "@slug@host", "slug@host" or an https URL to the actor IRI.
pub async fn resolve_handle(state: &AppState, handle: &str) -> Result<String, &'static str> {
    let h = handle.trim();
    if h.starts_with("https://") || h.starts_with("http://") {
        return Ok(h.to_string());
    }
    let h = h.trim_start_matches('@');
    let (user, host) = h.split_once('@').ok_or("not found")?;
    if user.is_empty() || host.is_empty() || host.contains('/') {
        return Err("not found");
    }
    let config = super::active(state).await.ok_or("federation is off")?;
    // Plain http only between development instances on one machine.
    let local = host.starts_with("127.") || host.starts_with("localhost");
    let scheme = if config.allow_private && local { "http" } else { "https" };
    let url = format!("{scheme}://{host}/.well-known/webfinger?resource=acct:{user}@{host}");
    let jrd = net::get_plain_json(state, &url).await.map_err(|_| "not found")?;
    jrd.get("links")
        .and_then(Value::as_array)
        .and_then(|links| {
            links.iter().find(|l| {
                l.get("rel").and_then(Value::as_str) == Some("self")
                    && l.get("type").and_then(Value::as_str).is_some_and(|t| t.contains("activity+json") || t.contains("ld+json"))
            })
        })
        .and_then(|l| l.get("href").and_then(Value::as_str))
        .map(str::to_string)
        .ok_or("not found")
}

/// The Follow the instance actor sends for a remote shelf (one per shelf).
/// Each gets a fresh id: the other side drops activity ids it has seen, so
/// following again after an unfollow must not reuse one. Undo embeds the
/// Follow, and receivers match it on actor and object.
pub fn follow_activity(c: &super::FedConfig, target: &str) -> Value {
    json!({
        "@context": objects::context(),
        "id": format!("{}/ap/follows/{}", c.base, crate::books::new_uuid()),
        "type": "Follow",
        "actor": c.instance_actor(),
        "object": target,
    })
}

pub async fn follow(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<FollowRequest>,
) -> Result<(StatusCode, Json<FollowItem>), Response> {
    super::active(&state).await.ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "federation is off"))?;
    let iri = resolve_handle(&state, &req.handle).await.map_err(|e| err(StatusCode::UNPROCESSABLE_ENTITY, e))?;
    let domain = super::domain_of(&iri).ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "not found"))?;
    // An instance the admin has not decided on: the follow is recorded and
    // held until it is allowed.
    let held = !super::domain_allowed(&state, &domain).await;
    if held && !super::requests::undecided(&state, &domain).await {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "domain not allowed"));
    }
    let actor = fetch_actor(&state, &iri).await.map_err(|e| {
        tracing::info!("fed: could not fetch {iri}: {e}");
        err(StatusCode::UNPROCESSABLE_ENTITY, "not found")
    })?;
    if actor.kind.as_deref() != Some("Service") || actor.inbox.is_none() {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "not a shelf"));
    }
    let existing: Vec<String> = sqlx::query_scalar("SELECT state FROM ap_follows WHERE remote_actor_iri = $1")
        .bind(&iri)
        .fetch_all(&state.db)
        .await
        .map_err(internal)?;
    let mine: Option<String> = sqlx::query_scalar("SELECT state FROM ap_follows WHERE user_id = $1 AND remote_actor_iri = $2")
        .bind(user.0.id)
        .bind(&iri)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    if matches!(mine.as_deref(), Some("pending") | Some("accepted")) {
        return Err(err(StatusCode::CONFLICT, "already following"));
    }
    // Another user already follows it: share that follow.
    let shared_state = if existing.iter().any(|s| s == "accepted") {
        Some("accepted")
    } else if existing.iter().any(|s| s == "pending") {
        Some("pending")
    } else {
        None
    };
    let new_state = shared_state.unwrap_or("pending");
    // sent_at follows the shared follow; a new one is stamped when sent.
    sqlx::query(
        "INSERT INTO ap_follows (user_id, remote_actor_iri, state, created_at, sent_at)
         VALUES ($1, $2, $3, $4, (SELECT MAX(sent_at) FROM ap_follows WHERE remote_actor_iri = $2))
         ON CONFLICT (user_id, remote_actor_iri) DO UPDATE
             SET state = excluded.state, created_at = excluded.created_at, sent_at = excluded.sent_at",
    )
    .bind(user.0.id)
    .bind(&iri)
    .bind(new_state)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(internal)?;
    if held {
        let shelf = actor.preferred_username.as_deref().map_or(iri.clone(), |u| format!("@{u}@{domain}"));
        super::requests::record(&state, &domain, "out", &shelf, &user.0.username).await;
    } else if shared_state.is_none() {
        super::requests::send_follow(&state, &iri).await.map_err(internal)?;
    }
    crate::audit::log(&state, crate::audit::by(&user.0), "fed.follow", json!({ "actor": iri, "name": actor.name })).await;
    let item = follow_items(&state, user.0.id, Some(&iri)).await?.into_iter().next().ok_or_else(|| internal("follow vanished"))?;
    Ok((StatusCode::CREATED, Json(item)))
}

#[derive(Deserialize)]
pub struct ActorParam {
    actor: String,
}

pub async fn unfollow(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ActorParam>,
) -> Result<StatusCode, Response> {
    let removed = sqlx::query("DELETE FROM ap_follows WHERE user_id = $1 AND remote_actor_iri = $2")
        .bind(user.0.id)
        .bind(&q.actor)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if removed.rows_affected() == 0 {
        return Err(err(StatusCode::NOT_FOUND, "not found"));
    }
    end_follow_if_unused(&state, &q.actor).await.map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "fed.unfollow", json!({ "actor": q.actor })).await;
    Ok(StatusCode::NO_CONTENT)
}

/// When no local user follows a shelf any more: Undo{Follow} and forget its books.
pub async fn end_follow_if_unused(state: &AppState, shelf_actor: &str) -> anyhow::Result<()> {
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_follows WHERE remote_actor_iri = $1")
        .bind(shelf_actor)
        .fetch_one(&state.db)
        .await?;
    if left > 0 {
        return Ok(());
    }
    if let Some(c) = super::active(state).await {
        if let Some(inbox) = cached_actor(state, shelf_actor).await.and_then(|a| a.inbox) {
            let undo = json!({
                "@context": objects::context(),
                "id": super::activity_id(&c),
                "type": "Undo",
                "actor": c.instance_actor(),
                "object": follow_activity(&c, shelf_actor),
            });
            super::deliver::enqueue(state, &[inbox], &c.instance_actor(), &undo).await?;
        }
    }
    sqlx::query("DELETE FROM ap_remote_books WHERE shelf_actor_iri = $1").bind(shelf_actor).execute(&state.db).await?;
    Ok(())
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RemoteBook {
    object_iri: String,
    title: String,
    author: Option<String>,
    language: Option<String>,
    isbn: Option<String>,
    summary: Option<String>,
    license: String,
    license_source_url: String,
    author_death_year: Option<i64>,
    epub_size: i64,
    cover_url: Option<String>,
    published: Option<String>,
    imported_book_id: Option<i64>,
}

async fn user_follows(state: &AppState, user_id: i64, actor: &str) -> Result<bool, Response> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_follows WHERE user_id = $1 AND remote_actor_iri = $2")
        .bind(user_id)
        .bind(actor)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    Ok(n > 0)
}

pub async fn books(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ActorParam>,
) -> Result<Json<Vec<RemoteBook>>, Response> {
    if !user_follows(&state, user.0.id, &q.actor).await? {
        return Err(err(StatusCode::NOT_FOUND, "not found"));
    }
    // Opening a shelf is a good moment to notice that its instance is down.
    {
        let (state, actor) = (state.clone(), q.actor.clone());
        tokio::spawn(async move { super::requests::check_if_stale(&state, &actor, 10).await });
    }
    let books: Vec<RemoteBook> = sqlx::query_as(
        "SELECT r.object_iri, r.title, r.author, r.language, r.isbn, r.summary, r.license, r.license_source_url,
                r.author_death_year, r.epub_size, r.cover_url, r.published,
                (SELECT MIN(b.id) FROM books b WHERE b.owner_id = $1 AND b.fed_source = r.object_iri) AS imported_book_id
         FROM ap_remote_books r
         WHERE r.shelf_actor_iri = $2 AND r.deleted_at IS NULL
         ORDER BY r.received_at DESC, LOWER(r.title)",
    )
    .bind(user.0.id)
    .bind(&q.actor)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    Ok(Json(books))
}

#[derive(Deserialize)]
pub struct ImportRequest {
    object_iri: String,
}

/// Import a remote book into the user's library: download, check hash, size
/// and EPUB format, run the gate again, then store it as an ordinary book.
pub async fn import(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ImportRequest>,
) -> Result<(StatusCode, Json<crate::books::Book>), Response> {
    super::active(&state).await.ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "federation is off"))?;
    #[derive(sqlx::FromRow)]
    struct Row {
        shelf_actor_iri: String,
        title: String,
        license: String,
        license_source_url: String,
        author_death_year: Option<i64>,
        epub_url: String,
        epub_sha256: String,
        epub_size: i64,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT shelf_actor_iri, title, license, license_source_url, author_death_year, epub_url, epub_sha256, epub_size
         FROM ap_remote_books WHERE object_iri = $1 AND deleted_at IS NULL",
    )
    .bind(&req.object_iri)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    let row = row.ok_or_else(|| err(StatusCode::NOT_FOUND, "not found"))?;
    if !user_follows(&state, user.0.id, &row.shelf_actor_iri).await? {
        return Err(err(StatusCode::NOT_FOUND, "not found"));
    }
    let already: Option<i64> = sqlx::query_scalar("SELECT MIN(id) FROM books WHERE owner_id = $1 AND fed_source = $2")
        .bind(user.0.id)
        .bind(&req.object_iri)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if let Some(book_id) = already {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "already imported", "book_id": book_id }))).into_response());
    }
    let facts = LicenseFacts {
        license: Some(row.license.clone()),
        source_url: Some(row.license_source_url.clone()),
        author_death_year: row.author_death_year,
    };
    if let Err(reason) = license::federable(&facts, license::current_year()) {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "not federable", "reason": reason }))).into_response());
    }
    let limit = super::max_epub_bytes(&state).await;
    if row.epub_size as usize > limit {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "file too large"));
    }
    let bytes = net::get_bytes(&state, &row.epub_url, limit).await.map_err(|e| {
        tracing::warn!("fed: fetching {} failed: {e}", row.epub_url);
        err(StatusCode::BAD_GATEWAY, "fetch failed")
    })?;
    if bytes.len() as i64 != row.epub_size {
        tracing::warn!("fed: {} has wrong size ({} != {})", row.epub_url, bytes.len(), row.epub_size);
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "size mismatch"));
    }
    if crate::books::sha256_hex(&bytes) != row.epub_sha256 {
        tracing::warn!("fed: {} has wrong sha256", row.epub_url);
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "checksum mismatch"));
    }
    if !is_epub(&bytes) {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "not an epub"));
    }
    let book = match crate::books::store_book(&state, user.0.id, &bytes, &format!("{}.epub", row.title)).await? {
        Ok(book) => book,
        Err(e) => return Err(err(StatusCode::UNPROCESSABLE_ENTITY, &e)),
    };
    sqlx::query(
        "UPDATE books SET license = $1, license_source_url = $2, author_death_year = CAST($3 AS BIGINT), fed_source = $4
         WHERE id = $5",
    )
    .bind(&row.license)
    .bind(&row.license_source_url)
    .bind(row.author_death_year.map(|y| y.to_string()))
    .bind(&req.object_iri)
    .bind(book.id)
    .execute(&state.db)
    .await
    .map_err(internal)?;
    let book: crate::books::Book =
        sqlx::query_as(&format!("SELECT {} FROM books WHERE id = $1", crate::books::BOOK_COLUMNS))
            .bind(book.id)
            .fetch_one(&state.db)
            .await
            .map_err(internal)?;
    tracing::info!("fed: {} imported for user {}", req.object_iri, user.0.id);
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "book.fed_imported",
        json!({ "book_id": book.id, "title": book.title, "source": req.object_iri }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(book)))
}

/// A zip whose first entry is `mimetype` = application/epub+zip.
pub fn is_epub(bytes: &[u8]) -> bool {
    use std::io::Read;
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else { return false };
    let Ok(mut first) = archive.by_index(0) else { return false };
    if first.name() != "mimetype" {
        return false;
    }
    let mut s = String::new();
    first.read_to_string(&mut s).is_ok() && s.trim() == "application/epub+zip"
}
