//! The Kobo store beside Legejo. A device synced with Legejo normally loses
//! its Kobo account: the store, bought books and their highlights all live
//! behind the URLs Legejo now answers. With the admin setting on, Legejo
//! keeps them working:
//!
//! - initialization starts from Kobo's own resource list, fetched with the
//!   device's own Kobo token, and only the keys for Legejo's books point
//!   here; the store pages, products and sign-in go straight to Kobo;
//! - the library sync is Legejo's, with Kobo's own sync for the same device
//!   added to every round; Kobo's sync token rides inside Legejo's;
//! - a request about a book or collection that is not Legejo's is passed on
//!   to Kobo as it came, and Kobo's answer goes back as it was;
//! - covers of Kobo's books are fetched from Kobo's image server.
//!
//! The device takes some answers as the whole truth (the highlights of a
//! book): when Kobo cannot be reached for those, the device gets an error,
//! which it takes as "later", never an empty answer.

use crate::AppState;
use axum::body::Bytes;
use axum::extract::{OriginalUri, Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

/// Whether an admin has turned the Kobo store on beside Legejo.
pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "kobo_store_enabled", false).await.unwrap_or(false)
}

/// Requests to Kobo: no redirects followed (the device gets them as they
/// are), and short enough waits that a slow Kobo does not stall a sync.
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("an HTTP client")
    })
}

/// Headers that belong to the connection to Legejo, or say things about the
/// device Kobo has no need of, and are not passed on.
const NOT_PASSED_ON: [&str; 17] = [
    "host",
    "content-length",
    "connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "proxy-authorization",
    "proxy-connection",
    "accept-encoding",
    "cookie",
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-proto",
    "x-forwarded-host",
    "x-real-ip",
];

/// Headers of Kobo's answer that are the connection's, not the answer's.
const NOT_PASSED_BACK: [&str; 7] = ["connection", "keep-alive", "transfer-encoding", "content-length", "set-cookie", "trailer", "upgrade"];

/// Kobo's answer, read whole.
pub struct Upstream {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl IntoResponse for Upstream {
    fn into_response(self) -> Response {
        let mut response = (self.status, self.body).into_response();
        for (name, value) in &self.headers {
            if !NOT_PASSED_BACK.contains(&name.as_str()) {
                response.headers_mut().append(name.clone(), value.clone());
            }
        }
        response
    }
}

/// A request passed on to one of Kobo's services: the device's own headers
/// (its Kobo token among them), except Legejo's sync token, which Kobo
/// gets only as `kobo_sync_token` (Kobo's own, kept inside Legejo's).
pub async fn forward(
    base: &str,
    method: &Method,
    path_and_query: &str,
    headers: &HeaderMap,
    body: Bytes,
    kobo_sync_token: Option<&str>,
) -> anyhow::Result<Upstream> {
    let url = format!("{}{path_and_query}", base.trim_end_matches('/'));
    let method = reqwest::Method::from_bytes(method.as_str().as_bytes())?;
    let mut request = client().request(method, &url);
    for (name, value) in headers {
        let name = name.as_str();
        if NOT_PASSED_ON.contains(&name) || name == "x-kobo-synctoken" || name.starts_with("cf-") {
            continue;
        }
        request = request.header(name, value.as_bytes());
    }
    if let Some(token) = kobo_sync_token.filter(|t| !t.is_empty()) {
        request = request.header("x-kobo-synctoken", token);
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    let response = request.send().await?;
    let status = StatusCode::from_u16(response.status().as_u16())?;
    let mut back = HeaderMap::new();
    for (name, value) in response.headers() {
        if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(name.as_str().as_bytes()), HeaderValue::from_bytes(value.as_bytes())) {
            back.append(name, value);
        }
    }
    let body = response.bytes().await?;
    Ok(Upstream { status, headers: back, body })
}

/// Kobo's own resource list for this device, or None when Kobo cannot be
/// reached or answers with something else.
pub async fn resources(state: &AppState, headers: &HeaderMap) -> Option<Map<String, Value>> {
    let answer = forward(&state.settings.kobo.store, &Method::GET, "/v1/initialization", headers, Bytes::new(), None).await;
    let answer = match answer {
        Ok(answer) if answer.status.is_success() => answer,
        Ok(answer) => {
            tracing::warn!("kobo store: initialization answered HTTP {}; the store stays away this time", answer.status.as_u16());
            return None;
        }
        Err(e) => {
            tracing::warn!("kobo store: initialization could not be fetched: {e:#}; the store stays away this time");
            return None;
        }
    };
    let found = serde_json::from_slice::<Value>(&answer.body).ok().and_then(|v| v.get("Resources").and_then(Value::as_object).cloned());
    if found.is_none() {
        tracing::warn!("kobo store: initialization without resources; the store stays away this time");
    }
    found
}

/// What Kobo's own library sync gave for one round.
pub struct StoreSync {
    pub items: Vec<Value>,
    /// Kobo's next sync token; None when it sent none (the old one stands).
    pub token: Option<String>,
    /// Kobo's x-kobo-sync and the like, handed to the device as they came.
    pub headers: Vec<(&'static str, HeaderValue)>,
}

/// Kobo's library sync for this device, from where Kobo's own token says
/// the device is. An error leaves that token as it was, so that the next
/// round asks again from the same place; nothing is lost by a failure.
pub async fn sync(state: &AppState, headers: &HeaderMap, kobo_token: &str) -> anyhow::Result<StoreSync> {
    let answer = forward(&state.settings.kobo.store, &Method::GET, "/v1/library/sync", headers, Bytes::new(), Some(kobo_token)).await?;
    if !answer.status.is_success() {
        anyhow::bail!("HTTP {}", answer.status.as_u16());
    }
    let items: Vec<Value> = serde_json::from_slice(&answer.body).map_err(|e| anyhow::anyhow!("not a list: {e}"))?;
    let token = answer.headers.get("x-kobo-synctoken").and_then(|v| v.to_str().ok()).map(str::to_string);
    let headers = ["x-kobo-sync", "x-kobo-sync-mode", "x-kobo-recent-reads"]
        .into_iter()
        .filter_map(|name| answer.headers.get(name).map(|v| (name, v.clone())))
        .collect();
    Ok(StoreSync { items, token, headers })
}

// ---------------------------------------------------------------------------
// Requests about what is not Legejo's.

/// What a request under the token is about, when it names a book or a
/// collection by the device's id.
enum Subject<'a> {
    Book(&'a str),
    Tag(&'a str),
    Cover { id: &'a str, rest: &'a [&'a str] },
    /// Kobo's sign-in: the device gets Kobo's tokens, not Legejo's.
    Auth,
}

fn subject<'a>(method: &Method, segments: &'a [&'a str]) -> Option<Subject<'a>> {
    match segments {
        ["v1", "auth", "device" | "refresh"] => Some(Subject::Auth),
        ["v1", "library", "tags", tag] if method == Method::PUT || method == Method::DELETE => Some(Subject::Tag(tag)),
        ["v1", "library", "tags", tag, "items"] | ["v1", "library", "tags", tag, "items", "delete"] if method == Method::POST => {
            Some(Subject::Tag(tag))
        }
        ["v1", "library", id, "metadata"] if method == Method::GET => Some(Subject::Book(id)),
        ["v1", "library", id, "state"] if method == Method::GET || method == Method::PUT => Some(Subject::Book(id)),
        ["v1", "library", id] if method == Method::DELETE => Some(Subject::Book(id)),
        [id, rest @ ..] if method == Method::GET && (rest.len() == 4 || rest.len() == 5) && rest.last() == Some(&"image.jpg") => {
            Some(Subject::Cover { id, rest })
        }
        _ => None,
    }
}

/// Whether the user has this book in Legejo, as the device can have it.
pub async fn is_legejo_book(state: &AppState, user_id: i64, id: &str) -> bool {
    let clean: String = id.chars().filter(|c| *c != '-').collect();
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM books WHERE uuid = $1 AND owner_id = $2 AND format IN ('epub', 'cbz')")
        .bind(&clean)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map(|n| n > 0)
        // Unsure: Legejo's own handler answers, as without the store.
        .unwrap_or(true)
}

async fn is_legejo_tag(state: &AppState, user_id: i64, tag: &str) -> bool {
    let Some(shelf_id) = crate::kobo::parse_tag_id(tag) else { return false };
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM shelves WHERE id = $1 AND owner_id = $2")
        .bind(shelf_id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map(|n| n > 0)
        .unwrap_or(true)
}

/// The user behind the token in a path under /api/kobo/<token>/.
async fn user_of(state: &AppState, original: &Uri) -> Option<i64> {
    let token = original.path().strip_prefix("/api/kobo/")?.split('/').next()?;
    if token.is_empty() {
        return None;
    }
    sqlx::query_scalar("SELECT id FROM users WHERE kobo_token = $1").bind(token).fetch_optional(&state.db).await.ok().flatten()
}

/// Requests about a book or collection that is not Legejo's go to Kobo,
/// and covers of Kobo's books to Kobo's image server; the rest goes on to
/// Legejo's own handlers. Only with the setting on.
pub async fn beside(State(state): State<AppState>, original: OriginalUri, request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let Some(about) = subject(request.method(), &segments) else {
        return next.run(request).await;
    };
    if !enabled(&state).await {
        return next.run(request).await;
    }
    let Some(user_id) = user_of(&state, &original.0).await else {
        return next.run(request).await;
    };
    let kobos = match &about {
        Subject::Auth => true,
        Subject::Book(id) => !is_legejo_book(&state, user_id, id).await,
        Subject::Tag(tag) => !is_legejo_tag(&state, user_id, tag).await,
        Subject::Cover { id, .. } => !is_legejo_book(&state, user_id, id).await,
    };
    if !kobos {
        return next.run(request).await;
    }
    if let Subject::Cover { id, rest } = about {
        let to = format!("{}/{id}/{}", state.settings.kobo.images.trim_end_matches('/'), rest.join("/"));
        return (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, to)]).into_response();
    }
    pass_on(&state, request).await
}

/// The whole request to Kobo's store API, and its answer back. A Kobo that
/// cannot be reached is a 502: the device tries again later.
async fn pass_on(state: &AppState, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let path_and_query = parts.uri.path_and_query().map(|p| p.as_str().to_string()).unwrap_or_else(|| parts.uri.path().to_string());
    let body = match axum::body::to_bytes(body, 16 * 1024 * 1024).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    match forward(&state.settings.kobo.store, &parts.method, &path_and_query, &parts.headers, body, None).await {
        Ok(answer) => {
            tracing::info!("kobo store: {} {}: {}", parts.method, parts.uri.path(), answer.status.as_u16());
            answer.into_response()
        }
        Err(e) => {
            tracing::warn!("kobo store: {} {} could not be passed on: {e:#}", parts.method, parts.uri.path());
            (StatusCode::BAD_GATEWAY, Json(json!({ "error": "the Kobo store could not be reached" }))).into_response()
        }
    }
}

/// Anything under the token that Legejo does not answer itself: Kobo's,
/// with the setting on; otherwise the empty object the device takes as
/// "nothing here".
pub async fn fallback(State(state): State<AppState>, request: Request) -> Response {
    if enabled(&state).await {
        return pass_on(&state, request).await;
    }
    tracing::debug!("kobo: dummy response for {}", request.uri());
    Json(json!({})).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn about(method: Method, path: &str) -> Option<&'static str> {
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        subject(&method, &segments).map(|s| match s {
            Subject::Book(_) => "book",
            Subject::Tag(_) => "tag",
            Subject::Cover { .. } => "cover",
            Subject::Auth => "auth",
        })
    }

    #[test]
    fn requests_about_a_book_or_a_collection_are_told_apart() {
        assert_eq!(about(Method::GET, "/v1/library/abc/metadata"), Some("book"));
        assert_eq!(about(Method::PUT, "/v1/library/abc/state"), Some("book"));
        assert_eq!(about(Method::DELETE, "/v1/library/abc"), Some("book"));
        assert_eq!(about(Method::GET, "/v1/library/sync"), None);
        assert_eq!(about(Method::POST, "/v1/library/tags"), None);
        assert_eq!(about(Method::PUT, "/v1/library/tags/t1"), Some("tag"));
        assert_eq!(about(Method::POST, "/v1/library/tags/t1/items/delete"), Some("tag"));
        assert_eq!(about(Method::GET, "/abc/120/160/false/image.jpg"), Some("cover"));
        assert_eq!(about(Method::GET, "/abc/120/160/80/true/image.jpg"), Some("cover"));
        assert_eq!(about(Method::POST, "/v1/auth/device"), Some("auth"));
        assert_eq!(about(Method::GET, "/v1/initialization"), None);
        assert_eq!(about(Method::GET, "/v1/download/abc.kepub.epub"), None);
    }
}
