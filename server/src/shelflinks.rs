//! A shelf shared by a private link, and such a link followed.
//!
//! The owner of a shelf makes links to it, one per person it is given to.
//! A link is an address with a token in it that gives the shelf as an OPDS
//! feed without a login: for a reading app, or for a user of this or another
//! Legejo, who follows it and has the shelf among those shared with them.
//! A link can be taken back, and can be given a last day.
//!
//! Off until an admin turns it on: with it on, this server hands out books
//! to whoever holds a link, and fetches from the addresses its users follow.

use crate::auth::AuthUser;
use crate::books::{Book, BOOK_COLUMNS_B};
use crate::catalogs::{self, Entry};
use crate::fed::net;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;

const MAX_LINKS_PER_SHELF: i64 = 20;
const MAX_FOLLOWED: i64 = 50;
const MAX_NAME_CHARS: usize = 80;
/// The part of a link's address that says what it is.
const PATH_MARK: &str = "/api/opds/shared/";

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

fn not_found() -> Response {
    err(StatusCode::NOT_FOUND, "not found")
}

/// Whether an admin has turned shelves by link on for the instance.
pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "shelf_links_enabled", false).await.unwrap_or(false)
}

async fn require(state: &AppState) -> Result<(), Response> {
    if enabled(state).await {
        Ok(())
    } else {
        Err(not_found())
    }
}

// ---------------------------------------------------------------------------
// The owner's side: the links to a shelf.

#[derive(Serialize, sqlx::FromRow)]
pub struct Link {
    id: i64,
    name: String,
    /// The secret part of the address; the web app puts the address together.
    token: String,
    created_at: String,
    expires_at: Option<String>,
    last_used_at: Option<String>,
}

async fn own_shelf(state: &AppState, user_id: i64, shelf_id: i64) -> Result<String, Response> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM shelves WHERE id = $1 AND owner_id = $2")
        .bind(shelf_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    name.ok_or_else(not_found)
}

/// GET /api/shelves/{id}/links
pub async fn list(State(state): State<AppState>, user: AuthUser, Path(shelf_id): Path<i64>) -> Result<Json<Vec<Link>>, Response> {
    require(&state).await?;
    own_shelf(&state, user.0.id, shelf_id).await?;
    let links: Vec<Link> = sqlx::query_as(
        "SELECT id, name, token, created_at, expires_at, last_used_at FROM shelf_links WHERE shelf_id = $1 ORDER BY id",
    )
    .bind(shelf_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    Ok(Json(links))
}

#[derive(Deserialize)]
pub struct CreateLink {
    /// Who it is for, in the owner's words.
    name: String,
    /// Days until it stops working; forever when left out.
    days: Option<i64>,
}

/// POST /api/shelves/{id}/links
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(shelf_id): Path<i64>,
    Json(req): Json<CreateLink>,
) -> Result<(StatusCode, Json<Link>), Response> {
    require(&state).await?;
    let shelf = own_shelf(&state, user.0.id, shelf_id).await?;
    let name: String = req.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "name missing or too long"));
    }
    let expires_at = match req.days {
        None => None,
        Some(days) if (1..=3650).contains(&days) => Some(crate::db::ts_in_hours(days * 24)),
        Some(_) => return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "days out of range")),
    };
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelf_links WHERE shelf_id = $1")
        .bind(shelf_id)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if count >= MAX_LINKS_PER_SHELF {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "too many links"));
    }
    // 256 bits from the system's generator: not to be guessed.
    let token = format!("{}{}", crate::books::new_uuid(), crate::books::new_uuid());
    let now = crate::db::now_ts();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO shelf_links (shelf_id, token, name, created_at, expires_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(shelf_id)
    .bind(&token)
    .bind(&name)
    .bind(&now)
    .bind(&expires_at)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "shelf.link_created", json!({ "shelf_id": shelf_id, "shelf": shelf, "name": name })).await;
    Ok((StatusCode::CREATED, Json(Link { id, name, token, created_at: now, expires_at, last_used_at: None })))
}

/// DELETE /api/shelves/{id}/links/{link}: the address stops working at once.
pub async fn remove(State(state): State<AppState>, user: AuthUser, Path((shelf_id, link_id)): Path<(i64, i64)>) -> Result<StatusCode, Response> {
    require(&state).await?;
    let shelf = own_shelf(&state, user.0.id, shelf_id).await?;
    let name: Option<String> = sqlx::query_scalar("DELETE FROM shelf_links WHERE id = $1 AND shelf_id = $2 RETURNING name")
        .bind(link_id)
        .bind(shelf_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    let name = name.ok_or_else(not_found)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "shelf.link_removed", json!({ "shelf_id": shelf_id, "shelf": shelf, "name": name })).await;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// What a link gives: the shelf as an OPDS feed, its covers and its files.
// No login; the token is the key. An unknown, removed or expired token is
// simply not there.

struct Shared {
    link_id: i64,
    shelf_id: i64,
    owner_id: i64,
    name: String,
    description: Option<String>,
    owner: String,
}

async fn shared(state: &AppState, token: &str) -> Result<Shared, Response> {
    if !enabled(state).await || token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let row: Option<(i64, i64, i64, String, Option<String>, String, Option<String>)> = sqlx::query_as(
        "SELECT l.id, s.id, s.owner_id, s.name, s.description, u.username, l.expires_at
         FROM shelf_links l JOIN shelves s ON s.id = l.shelf_id JOIN users u ON u.id = s.owner_id
         WHERE l.token = $1",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    let Some((link_id, shelf_id, owner_id, name, description, owner, expires_at)) = row else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    if expires_at.is_some_and(|at| at <= crate::db::now_ts()) {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    Ok(Shared { link_id, shelf_id, owner_id, name, description, owner })
}

/// GET /api/opds/shared/{token}
pub async fn feed(State(state): State<AppState>, Path(token): Path<String>) -> Result<Response, Response> {
    let shared = shared(&state, &token).await?;
    let mut books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1 AND b.owner_id = $2
         ORDER BY sb.added_at DESC, b.id DESC"
    ))
    .bind(shared.shelf_id)
    .bind(shared.owner_id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let _ = sqlx::query("UPDATE shelf_links SET last_used_at = $1 WHERE id = $2")
        .bind(crate::db::now_ts())
        .bind(shared.link_id)
        .execute(&state.db)
        .await;

    let base = format!("{PATH_MARK}{token}");
    let esc = crate::opds::esc;
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog">
<id>legejo:shared-shelf:{id}</id>
<title>{title}</title>
<updated>{updated}</updated>
<author><name>{owner}</name></author>
<generator uri="https://github.com/jularboolean/legejo">Legejo</generator>
<link rel="self" href="{href}" type="{kind}"/>
<link rel="start" href="{href}" type="{kind}"/>
"#,
        id = shared.shelf_id,
        title = esc(&shared.name),
        updated = crate::opds::now(),
        owner = esc(&shared.owner),
        href = esc(&base),
        kind = crate::opds::ACQ_TYPE,
    );
    if let Some(description) = shared.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        xml.push_str(&format!("<subtitle>{}</subtitle>\n", esc(description)));
    }
    for book in &mut books {
        // Without the owner's rating: that is theirs, not the book's.
        book.rating = None;
        xml.push_str(&crate::opds::book_entry_under(book, &base, false));
    }
    xml.push_str("</feed>\n");
    let mut response = crate::opds::feed_response(crate::opds::ACQ_TYPE, xml);
    // The address is a secret: it is not to be kept by caches on the way.
    response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("private, no-store"));
    Ok(response)
}

/// The book's uuid, cover type and title, when it stands on the shared shelf.
async fn shared_book(state: &AppState, token: &str, book_id: i64) -> Result<(String, Option<String>, String), Response> {
    let shared = shared(state, token).await?;
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT b.uuid, b.cover_mime, b.title FROM books b JOIN shelf_books sb ON sb.book_id = b.id
         WHERE b.id = $1 AND sb.shelf_id = $2 AND b.owner_id = $3",
    )
    .bind(book_id)
    .bind(shared.shelf_id)
    .bind(shared.owner_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    row.ok_or_else(|| StatusCode::NOT_FOUND.into_response())
}

/// GET /api/opds/shared/{token}/books/{id}/cover
pub async fn cover(State(state): State<AppState>, Path((token, id)): Path<(String, i64)>) -> Result<Response, Response> {
    let (uuid, cover_mime, _) = shared_book(&state, &token, id).await?;
    let mime = cover_mime.ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    let data = tokio::fs::read(state.data_dir.join("covers").join(&uuid)).await.map_err(|_| StatusCode::NOT_FOUND.into_response())?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=3600".to_string())], data).into_response())
}

/// GET /api/opds/shared/{token}/books/{id}/file
pub async fn file(State(state): State<AppState>, Path((token, id)): Path<(String, i64)>) -> Result<Response, Response> {
    let (uuid, _, title) = shared_book(&state, &token, id).await?;
    let (path, format) = crate::books::book_file(&state, &uuid).await;
    let data = tokio::fs::read(path).await.map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, format.mime().to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", crate::books::download_name(&title, format))),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
        ],
        data,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// The other end: links a user here follows.

#[derive(Serialize, sqlx::FromRow)]
pub struct Linked {
    id: i64,
    title: String,
    description: Option<String>,
    /// The instance the shelf is on, for telling shelves apart. Never the
    /// address itself: it is a secret, and is not handed back to the page.
    #[sqlx(skip)]
    host: String,
    #[serde(skip)]
    url: String,
    created_at: String,
}

fn host_of(url: &str) -> String {
    Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default()
}

/// The address as it is kept: a link to a shelf and nothing after it.
fn link_address(raw: &str) -> Option<String> {
    let mut url = raw.trim().to_string();
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let parsed = Url::parse(&url).ok()?;
    let path = parsed.path();
    let token = path.strip_prefix(PATH_MARK)?.trim_end_matches('/');
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let port = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
    Some(format!("{}://{}{port}{PATH_MARK}{token}", parsed.scheme(), parsed.host_str()?))
}

async fn followed(state: &AppState, user_id: i64, id: i64) -> Result<Linked, Response> {
    let row: Option<Linked> = sqlx::query_as("SELECT id, title, description, url, created_at FROM linked_shelves WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    let mut linked = row.ok_or_else(not_found)?;
    linked.host = host_of(&linked.url);
    Ok(linked)
}

/// GET /api/linked-shelves
pub async fn linked_list(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<Linked>>, Response> {
    require(&state).await?;
    let mut rows: Vec<Linked> = sqlx::query_as("SELECT id, title, description, url, created_at FROM linked_shelves WHERE user_id = $1 ORDER BY title, id")
        .bind(user.0.id)
        .fetch_all(&state.db)
        .await
        .map_err(internal)?;
    for row in &mut rows {
        row.host = host_of(&row.url);
    }
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct Follow {
    url: String,
}

/// POST /api/linked-shelves: the link is read first, and only one that
/// answers as a shelf is kept.
pub async fn follow(State(state): State<AppState>, user: AuthUser, Json(req): Json<Follow>) -> Result<(StatusCode, Json<Linked>), Response> {
    require(&state).await?;
    let Some(url) = link_address(&req.url) else {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "not a shelf link"));
    };
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM linked_shelves WHERE user_id = $1 AND url = $2")
        .bind(user.0.id)
        .bind(&url)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    if let Some(id) = existing {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "already following", "id": id }))).into_response());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM linked_shelves WHERE user_id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if count >= MAX_FOLLOWED {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "too many shelves"));
    }
    let (feed, _) = catalogs::fetch_feed(&state, &url).await?;
    let title = match feed.title.chars().take(120).collect::<String>() {
        t if t.is_empty() => host_of(&url),
        t => t,
    };
    let now = crate::db::now_ts();
    let id: i64 = sqlx::query_scalar("INSERT INTO linked_shelves (user_id, url, title, description, created_at) VALUES ($1, $2, $3, $4, $5) RETURNING id")
        .bind(user.0.id)
        .bind(&url)
        .bind(&title)
        .bind(&feed.subtitle)
        .bind(&now)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "shelf.link_followed", json!({ "shelf": title, "host": host_of(&url) })).await;
    Ok((StatusCode::CREATED, Json(Linked { id, title, description: feed.subtitle, host: host_of(&url), url, created_at: now })))
}

/// DELETE /api/linked-shelves/{id}
pub async fn unfollow(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    require(&state).await?;
    let done = sqlx::query("DELETE FROM linked_shelves WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if done.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct LinkedPage {
    shelf: Linked,
    books: Vec<Entry>,
}

/// GET /api/linked-shelves/{id}: the shelf as it is now, read from its link.
/// What it is called and says about itself is kept up to date on the way.
pub async fn linked_shelf(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<Json<LinkedPage>, Response> {
    require(&state).await?;
    let mut shelf = followed(&state, user.0.id, id).await?;
    let (feed, _) = catalogs::fetch_feed(&state, &shelf.url).await?;
    let title: String = feed.title.chars().take(120).collect();
    if !title.is_empty() && (title != shelf.title || feed.subtitle != shelf.description) {
        let _ = sqlx::query("UPDATE linked_shelves SET title = $1, description = $2 WHERE id = $3")
            .bind(&title)
            .bind(&feed.subtitle)
            .bind(id)
            .execute(&state.db)
            .await;
        shelf.title = title;
        shelf.description = feed.subtitle.clone();
    }
    Ok(Json(LinkedPage { shelf, books: feed.entries }))
}

/// An address handed back by the page must lie under the link it was read
/// from: the link is no way to make this server fetch from anywhere else.
fn under(shelf: &Linked, href: &str) -> Result<(), Response> {
    if href.starts_with(&format!("{}/", shelf.url)) {
        Ok(())
    } else {
        Err(err(StatusCode::UNPROCESSABLE_ENTITY, "address not of this shelf"))
    }
}

#[derive(Deserialize)]
pub struct ImageParams {
    url: String,
}

/// GET /api/linked-shelves/{id}/image: a cover, by way of this server, so
/// that the browser never holds the link's address.
pub async fn linked_image(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>, Query(params): Query<ImageParams>) -> Result<Response, Response> {
    require(&state).await?;
    let shelf = followed(&state, user.0.id, id).await?;
    under(&shelf, &params.url)?;
    let got = net::get_public(&state, &params.url, "image/*", catalogs::IMAGE_LIMIT, catalogs::FEED_TIMEOUT)
        .await
        .map_err(|e| catalogs::fetch_failed(&shelf.host, e))?;
    let mime = got.content_type.as_deref().unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if !matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/webp" | "image/gif") {
        return Err(err(StatusCode::BAD_GATEWAY, "not an image"));
    }
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=86400".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        got.bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct ImportRequest {
    href: String,
    title: String,
    format: String,
}

/// POST /api/linked-shelves/{id}/import: fetch the file and store it as any
/// uploaded book. A file the user already has is not added again.
pub async fn linked_import(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<ImportRequest>,
) -> Result<(StatusCode, Json<Book>), Response> {
    require(&state).await?;
    let shelf = followed(&state, user.0.id, id).await?;
    under(&shelf, &req.href)?;
    let ext = match req.format.as_str() {
        "epub" | "pdf" | "cbz" => req.format.as_str(),
        _ => return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "unknown format")),
    };
    let accept = "application/epub+zip, application/pdf, */*;q=0.5";
    let got = net::get_public(&state, &req.href, accept, state.settings.max_upload_bytes, catalogs::FILE_TIMEOUT)
        .await
        .map_err(|e| catalogs::fetch_failed(&shelf.host, e))?;
    let sha = crate::books::sha256_hex(&got.bytes);
    let same: Option<i64> = sqlx::query_scalar("SELECT MIN(id) FROM books WHERE owner_id = $1 AND (file_sha256 = $2 OR upload_sha256 = $2)")
        .bind(user.0.id)
        .bind(&sha)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if let Some(book_id) = same {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "already in library", "book_id": book_id }))).into_response());
    }
    let title: String = req.title.trim().chars().filter(|c| !matches!(c, '/' | '\\')).take(200).collect();
    let filename = format!("{}.{ext}", if title.is_empty() { "book" } else { &title });
    let book = match crate::books::store_book(&state, user.0.id, &got.bytes, &filename).await? {
        Ok(book) => book,
        Err(e) => return Err(err(StatusCode::UNPROCESSABLE_ENTITY, &e)),
    };
    crate::audit::log(&state, crate::audit::by(&user.0), "book.link_imported", json!({ "book_id": book.id, "title": book.title, "shelf": shelf.title })).await;
    Ok((StatusCode::CREATED, Json(book)))
}

#[cfg(test)]
mod tests {
    use super::link_address;

    #[test]
    fn a_link_is_kept_as_its_bare_address() {
        let token = "0123456789abcdef".repeat(4);
        let bare = format!("https://books.example.org/api/opds/shared/{token}");
        assert_eq!(link_address(&bare).as_deref(), Some(bare.as_str()));
        assert_eq!(link_address(&format!("  {bare}/ ")).as_deref(), Some(bare.as_str()));
        assert_eq!(link_address(&format!("books.example.org/api/opds/shared/{token}")).as_deref(), Some(bare.as_str()));
        assert_eq!(link_address(&format!("{bare}?x=1#y")).as_deref(), Some(bare.as_str()));
        assert_eq!(
            link_address(&format!("http://127.0.0.1:3001/api/opds/shared/{token}")).as_deref(),
            Some(format!("http://127.0.0.1:3001/api/opds/shared/{token}").as_str())
        );
        // Not links to a shelf.
        assert_eq!(link_address("https://books.example.org/api/opds"), None);
        assert_eq!(link_address("https://books.example.org/api/opds/shared/short"), None);
        assert_eq!(link_address(&format!("https://books.example.org/api/opds/shared/{token}/books/1/file")), None);
        assert_eq!(link_address(&format!("https://books.example.org/elsewhere/{token}")), None);
        assert_eq!(link_address(""), None);
    }
}
