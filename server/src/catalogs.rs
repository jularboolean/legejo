//! Other libraries' OPDS catalogs: a user adds a catalog by its address,
//! browses and searches it from Legejo, and fetches a book into the library.
//!
//! The server fetches from addresses its users give it, so this is off until
//! an admin turns it on for the instance, and then off for each user until
//! they turn it on for themselves. Until both have, every endpoint here
//! answers 404.
//!
//! Legejo reads OPDS 1.x (Atom). A feed is fetched on every page view and
//! nothing of it is stored; only the catalog's address, title and search
//! address are. Every address in a feed comes from outside, so feeds, covers
//! and books all go through the checked client in fed/net.rs.

use crate::auth::AuthUser;
use crate::fed::net::{self, FetchError};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

const FEED_LIMIT: usize = 5 * 1024 * 1024;
const IMAGE_LIMIT: usize = 3 * 1024 * 1024;
const FEED_TIMEOUT: Duration = Duration::from_secs(15);
const FILE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CATALOGS: i64 = 30;
const MAX_ENTRIES: usize = 500;
const SUMMARY_CHARS: usize = 600;
const ATOM: &str = "application/atom+xml;profile=opds-catalog, application/atom+xml, application/xml;q=0.8";

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

/// A fetch that failed, as the answer to the user: an address Legejo will
/// not go to is the user's to correct, the rest is the other server's.
fn fetch_failed(url: &str, e: FetchError) -> Response {
    tracing::info!("catalog: fetching {url} failed: {e}");
    match e {
        FetchError::Blocked(_) => err(StatusCode::UNPROCESSABLE_ENTITY, "address not allowed"),
        FetchError::TooLarge => err(StatusCode::BAD_GATEWAY, "too large"),
        // The catalog answered, and the answer was no: that is not the same
        // as a catalog that cannot be reached, and trying again will not help.
        FetchError::Status(401 | 403) => err(StatusCode::BAD_GATEWAY, "refused"),
        FetchError::Status(404 | 410) => err(StatusCode::BAD_GATEWAY, "missing"),
        FetchError::Status(500..=599) => err(StatusCode::BAD_GATEWAY, "busy"),
        _ => err(StatusCode::BAD_GATEWAY, "fetch failed"),
    }
}

/// Whether an admin has turned catalogs on for the instance.
pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "catalogs_enabled", false).await.unwrap_or(false)
}

/// Whether `user_id` browses catalogs: on for the instance, and turned on
/// by the user.
pub async fn allowed(state: &AppState, user_id: i64) -> bool {
    enabled(state).await
        && sqlx::query_scalar::<_, i64>("SELECT catalogs FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .is_some_and(|on| on != 0)
}

async fn require(state: &AppState, user: &AuthUser) -> Result<(), Response> {
    if allowed(state, user.0.id).await {
        Ok(())
    } else {
        Err(err(StatusCode::NOT_FOUND, "not found"))
    }
}

// ---------------------------------------------------------------------------
// XML, as a small tree. Names are local names: the feeds mix Atom, OPDS,
// Dublin Core and XHTML, and none of the elements read here clash.

#[derive(Debug, Default)]
struct El {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<Node>,
}

#[derive(Debug)]
enum Node {
    El(El),
    Text(String),
}

impl El {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.kids.iter().filter_map(move |k| match k {
            Node::El(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    fn child(&self, name: &str) -> Option<&El> {
        self.kids.iter().find_map(|k| match k {
            Node::El(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    /// All text below this element, with a space between elements.
    fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for k in &self.kids {
            match k {
                Node::Text(t) => out.push_str(t),
                Node::El(e) => {
                    out.push(' ');
                    e.collect_text(out);
                    out.push(' ');
                }
            }
        }
    }

    fn descendants<'a>(&'a self, name: &str, found: &mut Vec<&'a El>) {
        for k in &self.kids {
            if let Node::El(e) = k {
                if e.name == name {
                    found.push(e);
                }
                e.descendants(name, found);
            }
        }
    }
}

fn parse_xml(bytes: &[u8]) -> Result<El, String> {
    use xml::reader::XmlEvent;
    let mut reader = xml::ParserConfig::new()
        .cdata_to_characters(true)
        .ignore_comments(true)
        .create_reader(bytes);
    let mut stack: Vec<El> = Vec::new();
    loop {
        match reader.next().map_err(|e| e.to_string())? {
            XmlEvent::StartElement { name, attributes, .. } => {
                if stack.len() >= 64 {
                    return Err("nested too deep".into());
                }
                stack.push(El {
                    name: name.local_name,
                    attrs: attributes.into_iter().map(|a| (a.name.local_name, a.value)).collect(),
                    kids: Vec::new(),
                });
            }
            XmlEvent::EndElement { .. } => {
                let el = stack.pop().ok_or("unbalanced")?;
                match stack.last_mut() {
                    Some(parent) => parent.kids.push(Node::El(el)),
                    None => return Ok(el),
                }
            }
            XmlEvent::Characters(s) | XmlEvent::Whitespace(s) => {
                if let Some(top) = stack.last_mut() {
                    top.kids.push(Node::Text(s));
                }
            }
            XmlEvent::EndDocument => return Err("empty document".into()),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// The feed, as the web app gets it.

#[derive(Serialize, Debug, PartialEq)]
pub struct Feed {
    pub title: String,
    pub entries: Vec<Entry>,
    pub next: Option<String>,
    pub previous: Option<String>,
    /// The catalog's search: an OpenSearch description, or a template.
    #[serde(skip)]
    pub search: Option<SearchLink>,
}

#[derive(Debug, PartialEq)]
pub struct SearchLink {
    pub href: String,
    /// True when `href` is the template itself and not a description of it.
    pub template: bool,
}

/// One entry: a book, when it has files to fetch, or else a way further
/// into the catalog.
#[derive(Serialize, Debug, PartialEq, Default)]
pub struct Entry {
    pub title: String,
    pub authors: Vec<String>,
    pub summary: Option<String>,
    pub language: Option<String>,
    pub rights: Option<String>,
    pub cover: Option<String>,
    /// Another feed: a section of the catalog, or the book's own page in it.
    pub href: Option<String>,
    pub files: Vec<FileLink>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct FileLink {
    pub href: String,
    /// "epub", "pdf" or "cbz".
    pub format: &'static str,
    /// What the catalog calls this file, e.g. "EPUB3 (with images)".
    pub title: Option<String>,
    pub size: Option<i64>,
}

/// `href` as an absolute address. A catalog served over https may still
/// write http in its links; those are taken as https.
fn resolve(base: &Url, href: &str) -> Option<String> {
    let mut url = base.join(href.trim()).ok()?;
    if url.scheme() == "http" && base.scheme() == "https" {
        url.set_scheme("https").ok()?;
    }
    matches!(url.scheme(), "http" | "https").then(|| url.to_string())
}

fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn shorten(text: String) -> String {
    if text.chars().count() <= SUMMARY_CHARS {
        return text;
    }
    let cut: String = text.chars().take(SUMMARY_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// Text that carries HTML as text (type="html"): the tags go, and the
/// handful of entities that are common in descriptions are read.
fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

/// What an entry says about itself. A catalog that lists a whole record in
/// paragraphs (Project Gutenberg) has one that begins "Summary:"; that
/// paragraph is the description, and the rest is the record.
fn summary_of(entry: &El) -> Option<String> {
    let el = entry.child("summary").or_else(|| entry.child("content"))?;
    let text = match el.attr("type") {
        Some("xhtml") => {
            let mut paragraphs = Vec::new();
            el.descendants("p", &mut paragraphs);
            let summary = paragraphs.iter().map(|p| squeeze(&p.text())).find_map(|p| p.strip_prefix("Summary:").map(|s| s.trim().to_string()));
            summary.unwrap_or_else(|| squeeze(&el.text()))
        }
        Some("html") => squeeze(&strip_html(&el.text())),
        _ => squeeze(&el.text()),
    };
    (!text.is_empty()).then(|| shorten(text))
}

fn file_format(mime: &str) -> Option<&'static str> {
    match mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "application/epub+zip" => Some("epub"),
        "application/pdf" => Some("pdf"),
        "application/x-cbz" | "application/vnd.comicbook+zip" => Some("cbz"),
        _ => None,
    }
}

/// A link to a file that is handed over as it is. Buying, borrowing,
/// subscribing and samples are other things.
fn is_open_acquisition(rel: &str) -> bool {
    match rel.strip_prefix("http://opds-spec.org/acquisition") {
        Some("") | Some("/") | Some("/open-access") => true,
        _ => false,
    }
}

fn is_feed_type(mime: &str) -> bool {
    mime.trim().to_ascii_lowercase().starts_with("application/atom+xml")
}

fn parse_entry(entry: &El, base: &Url) -> Entry {
    let mut out = Entry {
        title: squeeze(&entry.child("title").map(El::text).unwrap_or_default()),
        authors: entry
            .children("author")
            .filter_map(|a| a.child("name"))
            .map(|n| squeeze(&n.text()))
            .filter(|n| !n.is_empty())
            .collect(),
        summary: summary_of(entry),
        language: entry.child("language").map(|l| squeeze(&l.text())).filter(|l| !l.is_empty()),
        rights: entry.child("rights").map(|r| squeeze(&r.text())).filter(|r| !r.is_empty()),
        ..Entry::default()
    };
    let mut thumbnail = None;
    let mut image = None;
    for link in entry.children("link") {
        let (Some(href), rel, mime) = (link.attr("href"), link.attr("rel").unwrap_or(""), link.attr("type").unwrap_or("")) else {
            continue;
        };
        if is_open_acquisition(rel) {
            if let (Some(format), Some(href)) = (file_format(mime), resolve(base, href)) {
                out.files.push(FileLink {
                    href,
                    format,
                    title: link.attr("title").map(squeeze).filter(|t| !t.is_empty()),
                    size: link.attr("length").and_then(|l| l.parse().ok()).filter(|l| *l > 0),
                });
            }
        } else if mime.starts_with("image/") {
            // "…/image/thumbnail", and the older "…/thumbnail" and "…/cover".
            if rel.ends_with("thumbnail") {
                thumbnail = thumbnail.or_else(|| resolve(base, href));
            } else if rel.ends_with("/image") || rel.ends_with("cover") {
                image = image.or_else(|| resolve(base, href));
            }
        } else if is_feed_type(mime) && !matches!(rel, "related" | "self") && out.href.is_none() {
            out.href = resolve(base, href);
        }
    }
    out.cover = thumbnail.or(image);
    out
}

/// An OPDS 1.x feed, or a single entry served as a document of its own.
pub fn parse_feed(bytes: &[u8], base: &Url) -> Result<Feed, String> {
    let root = parse_xml(bytes)?;
    if root.name == "entry" {
        let entry = parse_entry(&root, base);
        return Ok(Feed { title: entry.title.clone(), entries: vec![entry], next: None, previous: None, search: None });
    }
    if root.name != "feed" {
        return Err(format!("a <{}> document, not a feed", root.name));
    }
    let mut feed = Feed {
        title: squeeze(&root.child("title").map(El::text).unwrap_or_default()),
        entries: root.children("entry").take(MAX_ENTRIES).map(|e| parse_entry(e, base)).collect(),
        next: None,
        previous: None,
        search: None,
    };
    for link in root.children("link") {
        let (Some(href), Some(rel)) = (link.attr("href"), link.attr("rel")) else { continue };
        let mime = link.attr("type").unwrap_or("");
        match rel {
            "next" => feed.next = feed.next.or_else(|| resolve(base, href)),
            "previous" | "prev" => feed.previous = feed.previous.or_else(|| resolve(base, href)),
            "search" if feed.search.is_none() => {
                if mime.starts_with("application/opensearchdescription+xml") {
                    feed.search = resolve(base, href).map(|href| SearchLink { href, template: false });
                } else if is_feed_type(mime) && href.contains("{searchTerms}") {
                    // The braces do not survive Url::join; a template is kept as written.
                    feed.search = Some(SearchLink { href: absolute_template(base, href), template: true });
                }
            }
            _ => {}
        }
    }
    Ok(feed)
}

/// A search template may be relative, and cannot be joined as a URL because
/// of its braces.
fn absolute_template(base: &Url, template: &str) -> String {
    let template = template.trim();
    if template.starts_with("http://") || template.starts_with("https://") {
        template.to_string()
    } else if template.starts_with('/') {
        format!("{}{template}", base.origin().ascii_serialization())
    } else {
        let dir = base.as_str().rsplit_once('/').map(|(dir, _)| dir).unwrap_or(base.as_str());
        format!("{dir}/{template}")
    }
}

/// The Atom search template of an OpenSearch description document.
pub fn opensearch_template(bytes: &[u8], base: &Url) -> Option<String> {
    let root = parse_xml(bytes).ok()?;
    let template = root
        .children("Url")
        .find(|u| is_feed_type(u.attr("type").unwrap_or("")))
        .and_then(|u| u.attr("template"))?;
    template.contains("{searchTerms}").then(|| absolute_template(base, template))
}

/// The search address for `terms`. Optional parameters ({name?}) are left
/// out; the few required ones a template may have get their plain values.
pub fn fill_template(template: &str, terms: &str) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(terms.as_bytes()).collect::<String>().replace('+', "%20");
    let mut out = String::with_capacity(template.len() + encoded.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            rest = &rest[start..];
            break;
        };
        let name = &rest[start + 1..start + end];
        out.push_str(match name.rsplit(':').next().unwrap_or(name) {
            "searchTerms" => &encoded,
            "startPage" | "startIndex" => "1",
            "count" => "25",
            "language" => "*",
            "inputEncoding" | "outputEncoding" => "UTF-8",
            _ => "",
        });
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// As `resolve` does for links: an http template of an https catalog.
fn upgrade(url: String, base: &Url) -> String {
    match url.strip_prefix("http://") {
        Some(rest) if base.scheme() == "https" => format!("https://{rest}"),
        _ => url,
    }
}

async fn fetch_feed(state: &AppState, url: &str) -> Result<(Feed, Url), Response> {
    let mut got = net::get_public(state, url, ATOM, FEED_LIMIT, FEED_TIMEOUT).await;
    // A catalog whose own back end timed out (a search it had not made
    // before) often answers the second time.
    if matches!(got, Err(FetchError::Status(502..=504))) {
        tracing::info!("catalog: {url} answered with a gateway error, trying once more");
        tokio::time::sleep(Duration::from_millis(500)).await;
        got = net::get_public(state, url, ATOM, FEED_LIMIT, FEED_TIMEOUT).await;
    }
    let got = got.map_err(|e| fetch_failed(url, e))?;
    match parse_feed(&got.bytes, &got.url) {
        Ok(feed) => Ok((feed, got.url)),
        Err(e) => {
            tracing::info!("catalog: {url} is not an OPDS feed: {e}");
            Err(err(StatusCode::BAD_GATEWAY, "not a catalog"))
        }
    }
}

/// The template the catalog searches with, from the feed's search link.
async fn search_template(state: &AppState, feed: &Feed, base: &Url) -> Option<String> {
    let link = feed.search.as_ref()?;
    if link.template {
        return Some(upgrade(link.href.clone(), base));
    }
    let accept = "application/opensearchdescription+xml, application/xml";
    let got = net::get_public(state, &link.href, accept, FEED_LIMIT, FEED_TIMEOUT).await.ok()?;
    opensearch_template(&got.bytes, &got.url).map(|t| upgrade(t, base))
}

// ---------------------------------------------------------------------------
// The user's catalogs.

#[derive(Serialize, sqlx::FromRow)]
pub struct Catalog {
    id: i64,
    title: String,
    url: String,
    #[serde(skip)]
    search_template: Option<String>,
    #[sqlx(skip)]
    searchable: bool,
}

async fn owned(state: &AppState, user_id: i64, id: i64) -> Result<Catalog, Response> {
    let catalog: Option<Catalog> =
        sqlx::query_as("SELECT id, title, url, search_template FROM opds_catalogs WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(internal)?;
    let mut catalog = catalog.ok_or_else(|| err(StatusCode::NOT_FOUND, "not found"))?;
    catalog.searchable = catalog.search_template.is_some();
    Ok(catalog)
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<Catalog>>, Response> {
    require(&state, &user).await?;
    let mut catalogs: Vec<Catalog> =
        sqlx::query_as("SELECT id, title, url, search_template FROM opds_catalogs WHERE user_id = $1 ORDER BY title, id")
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await
            .map_err(internal)?;
    for c in &mut catalogs {
        c.searchable = c.search_template.is_some();
    }
    Ok(Json(catalogs))
}

#[derive(Deserialize)]
pub struct Create {
    url: String,
}

/// POST /api/catalogs: the address is fetched first, and only a feed that
/// reads as an OPDS catalog is added. Its title is the catalog's own.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<Create>,
) -> Result<(StatusCode, Json<Catalog>), Response> {
    require(&state, &user).await?;
    let mut url = req.url.trim().to_string();
    if url.is_empty() || url.len() > 2000 {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "address missing"));
    }
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM opds_catalogs WHERE user_id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
        .map_err(internal)?;
    if count >= MAX_CATALOGS {
        return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "too many catalogs"));
    }
    let (feed, base) = fetch_feed(&state, &url).await?;
    let template = search_template(&state, &feed, &base).await;
    let title = match feed.title.chars().take(120).collect::<String>() {
        t if t.is_empty() => base.host_str().unwrap_or("OPDS").to_string(),
        t => t,
    };
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM opds_catalogs WHERE user_id = $1 AND url = $2")
        .bind(user.0.id)
        .bind(&url)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;
    if let Some(id) = existing {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "already added", "id": id }))).into_response());
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO opds_catalogs (user_id, title, url, search_template) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user.0.id)
    .bind(&title)
    .bind(&url)
    .bind(&template)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    Ok((StatusCode::CREATED, Json(Catalog { id, title, url, searchable: template.is_some(), search_template: template })))
}

pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    require(&state, &user).await?;
    let done = sqlx::query("DELETE FROM opds_catalogs WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if done.rows_affected() == 0 {
        return Err(err(StatusCode::NOT_FOUND, "not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Browsing.

#[derive(Deserialize)]
pub struct FeedParams {
    /// A feed inside the catalog; the catalog's first page when left out.
    url: Option<String>,
    /// Search the catalog for this.
    q: Option<String>,
}

#[derive(Serialize)]
pub struct FeedPage {
    catalog: Catalog,
    /// The address this page was read from.
    url: String,
    #[serde(flatten)]
    feed: Feed,
}

/// GET /api/catalogs/{id}/feed
pub async fn feed(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(params): Query<FeedParams>,
) -> Result<Json<FeedPage>, Response> {
    require(&state, &user).await?;
    let catalog = owned(&state, user.0.id, id).await?;
    let terms = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    // A later page of a search has an address of its own; the terms then
    // only say what the page is a search for.
    let url = match (&params.url, terms) {
        (Some(url), _) => url.clone(),
        (None, Some(terms)) => {
            let template = catalog.search_template.as_deref().ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "no search"))?;
            fill_template(template, terms)
        }
        (None, None) => catalog.url.clone(),
    };
    let (feed, base) = fetch_feed(&state, &url).await?;
    Ok(Json(FeedPage { catalog, url: base.to_string(), feed }))
}

#[derive(Deserialize)]
pub struct ImageParams {
    url: String,
}

/// GET /api/catalogs/{id}/image: a cover from the catalog, by way of this
/// server, so the reader's browser never talks to the catalog itself. Only
/// the bitmap types are passed on: an SVG served from here could run script.
pub async fn image(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(params): Query<ImageParams>,
) -> Result<Response, Response> {
    require(&state, &user).await?;
    owned(&state, user.0.id, id).await?;
    let got = net::get_public(&state, &params.url, "image/*", IMAGE_LIMIT, FEED_TIMEOUT)
        .await
        .map_err(|e| fetch_failed(&params.url, e))?;
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

// ---------------------------------------------------------------------------
// Fetching a book.

#[derive(Deserialize)]
pub struct ImportRequest {
    href: String,
    /// The entry's title: the book's name until the file has said its own.
    title: String,
    /// "epub", "pdf" or "cbz", as the feed gave it.
    format: String,
}

/// POST /api/catalogs/{id}/import: download the file and store it as any
/// uploaded book. A file the user already has is not added again.
pub async fn import(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<ImportRequest>,
) -> Result<(StatusCode, Json<crate::books::Book>), Response> {
    require(&state, &user).await?;
    let catalog = owned(&state, user.0.id, id).await?;
    let ext = match req.format.as_str() {
        "epub" | "pdf" | "cbz" => req.format.as_str(),
        _ => return Err(err(StatusCode::UNPROCESSABLE_ENTITY, "unknown format")),
    };
    let accept = "application/epub+zip, application/pdf, */*;q=0.5";
    let got = net::get_public(&state, &req.href, accept, state.settings.max_upload_bytes, FILE_TIMEOUT)
        .await
        .map_err(|e| fetch_failed(&req.href, e))?;
    let sha = crate::books::sha256_hex(&got.bytes);
    let same: Option<i64> = sqlx::query_scalar(
        "SELECT MIN(id) FROM books WHERE owner_id = $1 AND (file_sha256 = $2 OR upload_sha256 = $2)",
    )
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
        Err(e) => {
            tracing::info!("catalog: {} is not a book: {e}", req.href);
            return Err(err(StatusCode::UNPROCESSABLE_ENTITY, &e));
        }
    };
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "book.catalog_imported",
        json!({ "book_id": book.id, "title": book.title, "catalog": catalog.title }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(book)))
}
