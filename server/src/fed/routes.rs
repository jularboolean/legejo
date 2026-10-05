//! Public federation endpoints: WebFinger, nodeinfo, actors, outboxes and
//! book objects. All of them answer 404 while federation is off.

use super::objects::{self, FedShelf, OutBook, FED_SHELF_SELECT};
use super::{FedConfig, AP_JSON};
use crate::license;
use crate::AppState;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/webfinger", get(webfinger))
        .route("/.well-known/nodeinfo", get(nodeinfo_links))
        .route("/nodeinfo/2.1", get(nodeinfo))
        .route("/ap/actor", get(instance_actor))
        .route("/ap/actor/outbox", get(empty_outbox))
        .route("/ap/inbox", post(super::inbox::post))
        .route("/ap/shelves/{slug}", get(shelf_actor))
        .route("/ap/shelves/{slug}/inbox", post(super::inbox::post))
        .route("/ap/shelves/{slug}/outbox", get(outbox))
        .route("/ap/shelves/{slug}/followers", get(followers))
        .route("/ap/shelves/{slug}/icon", get(shelf_icon))
        .route("/ap/books/{uuid}", get(book))
        .route("/ap/books/{uuid}/epub", get(book_epub))
        .route("/ap/books/{uuid}/cover", get(book_cover))
        .route("/f/{slug}", get(super::pages::shelf))
        .route("/f/{slug}/{uuid}", get(super::pages::book))
        .layer(DefaultBodyLimit::max(1024 * 1024))
}

pub fn ap_json(value: Value) -> Response {
    (
        [
            (header::CONTENT_TYPE, AP_JSON),
            (header::VARY, "Accept"),
            // private: a shared cache must never serve AP JSON to a browser or the reverse.
            (header::CACHE_CONTROL, "private, max-age=60"),
        ],
        value.to_string(),
    )
        .into_response()
}

pub fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

pub async fn on(state: &AppState) -> Result<FedConfig, Response> {
    super::active(state).await.ok_or_else(not_found)
}

pub fn wants_html(headers: &HeaderMap) -> bool {
    let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
    accept.contains("text/html") && !accept.contains("activity+json") && !accept.contains("ld+json")
}

pub async fn fed_shelf(state: &AppState, slug: &str) -> Result<FedShelf, Response> {
    let shelf: Option<FedShelf> = sqlx::query_as(&format!(
        "{FED_SHELF_SELECT} WHERE lower(s.ap_slug) = lower($1) AND s.visibility = 'federated'"
    ))
    .bind(slug)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
    shelf.ok_or_else(not_found)
}

#[derive(Deserialize)]
struct Resource {
    resource: String,
}

async fn webfinger(State(state): State<AppState>, Query(q): Query<Resource>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let acct = q.resource.strip_prefix("acct:").unwrap_or(&q.resource);
    let slug = if let Some((user, host)) = acct.trim_start_matches('@').split_once('@') {
        if !host.eq_ignore_ascii_case(&c.host) {
            return Err(not_found());
        }
        user.to_string()
    } else if let Some(slug) = c.slug_of(acct) {
        slug
    } else {
        return Err(not_found());
    };
    let shelf = fed_shelf(&state, &slug).await?;
    Ok((
        [(header::CONTENT_TYPE, "application/jrd+json"), (header::CACHE_CONTROL, "private, max-age=60")],
        objects::webfinger(&c, &shelf.ap_slug).to_string(),
    )
        .into_response())
}

async fn nodeinfo_links(State(state): State<AppState>) -> Result<Response, Response> {
    let c = on(&state).await?;
    Ok(axum::Json(json!({
        "links": [{ "rel": "http://nodeinfo.diaspora.software/ns/schema/2.1", "href": format!("{}/nodeinfo/2.1", c.base) }]
    }))
    .into_response())
}

async fn nodeinfo(State(state): State<AppState>) -> Result<Response, Response> {
    on(&state).await?;
    let shelves: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelves WHERE visibility = 'federated' AND ap_slug IS NOT NULL")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    let books: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_published").fetch_one(&state.db).await.unwrap_or(0);
    let contact = super::setting(&state, "federation_contact").await;
    let mode = super::mode(&state).await;
    Ok((
        [(header::CONTENT_TYPE, "application/json; profile=\"http://nodeinfo.diaspora.software/ns/schema/2.1#\"")],
        objects::nodeinfo(mode.as_str(), contact.as_deref(), shelves, books).to_string(),
    )
        .into_response())
}

async fn instance_actor(State(state): State<AppState>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let (_, pem) = super::sig::key_for(&state, &c.instance_actor())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
    Ok(ap_json(objects::instance_actor(&c, &pem)))
}

async fn empty_outbox(State(state): State<AppState>) -> Result<Response, Response> {
    let c = on(&state).await?;
    Ok(ap_json(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/outbox", c.instance_actor()),
        "type": "OrderedCollection", "totalItems": 0, "orderedItems": []
    })))
}

async fn shelf_actor(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    if wants_html(&headers) {
        return Ok(Redirect::to(&format!("/f/{}", shelf.ap_slug)).into_response());
    }
    let (_, pem) = super::sig::key_for(&state, &c.shelf_actor(&shelf.ap_slug))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
    Ok(ap_json(objects::shelf_actor(&c, &shelf, &pem)))
}

#[derive(Deserialize)]
struct PageParam {
    page: Option<usize>,
}

const PAGE_SIZE: usize = 20;

async fn outbox(State(state): State<AppState>, Path(slug): Path<String>, Query(q): Query<PageParam>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    let books = super::reconcile::shelf_books(&state, shelf.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())?;
    let actor = c.shelf_actor(&shelf.ap_slug);
    let id = format!("{actor}/outbox");
    let Some(page) = q.page else {
        return Ok(ap_json(json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "id": id,
            "type": "OrderedCollection",
            "totalItems": books.len(),
            "first": format!("{id}?page=1"),
            "last": format!("{id}?page={}", books.len().div_ceil(PAGE_SIZE).max(1)),
        })));
    };
    let page = page.max(1);
    let published: Vec<(i64, String)> = sqlx::query_as("SELECT book_id, published_at FROM ap_published WHERE shelf_id = $1")
        .bind(shelf.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let items: Vec<Value> = books
        .iter()
        .skip((page - 1) * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|p| {
            let mut object = objects::book_object(&c, &shelf.ap_slug, &OutBook { book: &p.book, sha256: &p.sha256 });
            let when = published.iter().find(|(id, _)| *id == p.book.id).map(|(_, t)| t.clone()).unwrap_or(p.book.created_at.clone());
            object["published"] = json!(when);
            let mut a = objects::activity(&c, "Create", &actor, object);
            a["id"] = json!(format!("{}/create", c.book_iri(&p.book.uuid)));
            if let Some(o) = a.as_object_mut() {
                o.remove("@context");
            }
            a
        })
        .collect();
    let mut body = json!({
        "@context": objects::context(),
        "id": format!("{id}?page={page}"),
        "type": "OrderedCollectionPage",
        "partOf": id,
        "orderedItems": items,
    });
    if page * PAGE_SIZE < books.len() {
        body["next"] = json!(format!("{id}?page={}", page + 1));
    }
    if page > 1 {
        body["prev"] = json!(format!("{id}?page={}", page - 1));
    }
    Ok(ap_json(body))
}

async fn followers(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ap_followers WHERE shelf_id = $1")
        .bind(shelf.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    // Only the count: who follows a shelf is nobody else's business.
    Ok(ap_json(json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}/followers", c.shelf_actor(&shelf.ap_slug)),
        "type": "OrderedCollection",
        "totalItems": n,
    })))
}

async fn shelf_icon(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Response, Response> {
    on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    let mime: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM shelves WHERE id = $1")
        .bind(shelf.id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| not_found())?;
    let mime = mime.ok_or_else(not_found)?;
    let data = tokio::fs::read(state.data_dir.join("shelf_covers").join(shelf.id.to_string())).await.map_err(|_| not_found())?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "public, max-age=3600".into())], data).into_response())
}

/// A book on some federated shelf, gated again (defence in depth).
pub struct FedBook {
    pub book: crate::books::Book,
    pub sha256: String,
    pub slug: String,
    pub cover_mime: Option<String>,
}

pub async fn fed_book(state: &AppState, uuid: &str) -> Result<FedBook, Response> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT b.id, s.ap_slug FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         JOIN shelves s ON s.id = sb.shelf_id
         WHERE b.uuid = $1 AND s.visibility = 'federated' AND s.ap_slug IS NOT NULL
         ORDER BY sb.added_at LIMIT 1",
    )
    .bind(uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| not_found())?;
    let (id, slug) = row.ok_or_else(not_found)?;
    let book: crate::books::Book = sqlx::query_as(&format!("SELECT {} FROM books WHERE id = $1", crate::books::BOOK_COLUMNS))
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| not_found())?;
    if let Err(reason) = license::federable(&book.license_facts(), license::current_year()) {
        tracing::error!(
            "fed: inconsistent database: {uuid} on federated shelf {slug} was requested but fails the license gate ({})",
            serde_json::to_string(&reason).unwrap_or_default()
        );
        return Err(not_found());
    }
    let (sha256, cover_mime): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT file_sha256, cover_mime FROM books WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|_| not_found())?;
    Ok(FedBook { book, sha256: sha256.ok_or_else(not_found)?, slug, cover_mime })
}

async fn book(State(state): State<AppState>, headers: HeaderMap, Path(uuid): Path<String>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let b = fed_book(&state, &uuid).await?;
    if wants_html(&headers) {
        return Ok(Redirect::to(&format!("/f/{}/{}", b.slug, uuid)).into_response());
    }
    let mut object = objects::book_object(&c, &b.slug, &OutBook { book: &b.book, sha256: &b.sha256 });
    object["published"] = json!(b.book.created_at);
    Ok(ap_json(object))
}

async fn book_epub(State(state): State<AppState>, Path(uuid): Path<String>) -> Result<Response, Response> {
    on(&state).await?;
    let b = fed_book(&state, &uuid).await?;
    // The original file, never the kepub.
    let data = tokio::fs::read(state.data_dir.join("books").join(format!("{uuid}.epub"))).await.map_err(|_| not_found())?;
    let safe: String = b.book.title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
    Ok((
        [
            (header::CONTENT_TYPE, "application/epub+zip".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{safe}.epub\"")),
            (header::CACHE_CONTROL, "public, max-age=300".into()),
        ],
        data,
    )
        .into_response())
}

/// The cover when the owner confirmed it is free; otherwise a generated one
/// (a cover is a separate work and may be under copyright of its own).
async fn book_cover(State(state): State<AppState>, Path(uuid): Path<String>) -> Result<Response, Response> {
    on(&state).await?;
    let b = fed_book(&state, &uuid).await?;
    if b.book.cover_is_free.as_bool() {
        if let Some(mime) = b.cover_mime.filter(|m| m.starts_with("image/")) {
            if let Ok(data) = tokio::fs::read(state.data_dir.join("covers").join(&uuid)).await {
                return Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "public, max-age=3600".into())], data).into_response());
            }
        }
    }
    Ok((
        [(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "public, max-age=3600")],
        generated_cover(&b.book.title, b.book.author.as_deref()),
    )
        .into_response())
}

/// Title and author on a plain colour derived from the title.
pub fn generated_cover(title: &str, author: Option<&str>) -> String {
    let hash = crate::books::sha256_hex(title.as_bytes());
    let hue = u32::from_str_radix(&hash[..4], 16).unwrap_or(0) % 360;
    let lines = wrap(title, 16);
    let mut text = String::new();
    for (i, line) in lines.iter().take(5).enumerate() {
        text.push_str(&format!(
            "<text x=\"200\" y=\"{}\" font-size=\"34\" font-weight=\"600\">{}</text>",
            200 + i * 44,
            objects::escape(line)
        ));
    }
    if let Some(a) = author {
        text.push_str(&format!("<text x=\"200\" y=\"500\" font-size=\"22\">{}</text>", objects::escape(a)));
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"600\" viewBox=\"0 0 400 600\">\
         <rect width=\"400\" height=\"600\" fill=\"hsl({hue},35%,32%)\"/>\
         <rect x=\"24\" y=\"24\" width=\"352\" height=\"552\" fill=\"none\" stroke=\"rgba(255,255,255,.35)\" stroke-width=\"2\"/>\
         <g fill=\"#fff\" font-family=\"Georgia, serif\" text-anchor=\"middle\">{text}</g></svg>"
    )
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
