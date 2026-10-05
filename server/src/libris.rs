//! Metadata lookup against Libris Xsearch, the Swedish national union
//! catalogue. Proxied server-side to avoid CORS and to control the
//! User-Agent sent to the National Library of Sweden (KB).

use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize)]
pub struct SearchParams {
    isbn: Option<String>,
    title: Option<String>,
    author: Option<String>,
}

#[derive(Serialize)]
pub struct Candidate {
    pub libris_id: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub creator: Option<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub language: Option<String>,
    pub isbn: Vec<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub records: i64,
    pub candidates: Vec<Candidate>,
}

fn disabled() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Libris lookup is disabled" })))
        .into_response()
}

async fn ensure_enabled(state: &AppState) -> Result<(), Response> {
    match crate::admin::libris_enabled(state).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(disabled()),
        Err(e) => Err(upstream_error(e.into())),
    }
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn upstream_error(e: anyhow::Error) -> Response {
    tracing::warn!("libris lookup failed: {e:#}");
    (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "error": "could not reach Libris" })))
        .into_response()
}

/// Xsearch values are sometimes strings, sometimes arrays of strings.
fn first_str(v: Option<&Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => a.iter().find_map(|x| x.as_str().map(String::from)),
        _ => None,
    }
}

fn all_str(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// Strip characters with meaning in the Xsearch query language.
fn sanitize(term: &str) -> String {
    term.chars()
        .map(|c| if "():\"".contains(c) { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Deserialize)]
pub struct SummaryParams {
    id: String,
}

/// Fetch the full XL record for a Libris ID and pull out its summary, when
/// one exists. Numeric bib IDs resolve via /resource/bib/, XL IDs directly.
pub async fn summary(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(params): Query<SummaryParams>,
) -> Result<Json<Value>, Response> {
    ensure_enabled(&state).await?;
    let id: String = params.id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if id.is_empty() {
        return Err(bad_request("missing id"));
    }
    let url = if id.chars().all(|c| c.is_ascii_digit()) {
        format!("https://libris.kb.se/resource/bib/{id}")
    } else {
        format!("https://libris.kb.se/{id}")
    };

    let response = state
        .http
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/ld+json")
        .send()
        .await
        .map_err(|e| upstream_error(e.into()))?;
    if !response.status().is_success() {
        return Ok(Json(serde_json::json!({ "summary": null })));
    }
    let body: Value = response.json().await.map_err(|e| upstream_error(e.into()))?;

    // The summary can sit on the Instance or the Work node; take the longest.
    let empty = Vec::new();
    let nodes = body["@graph"].as_array().unwrap_or(&empty);
    let best = nodes
        .iter()
        .chain(std::iter::once(&body))
        .filter_map(|node| node.get("summary")?.as_array())
        .flatten()
        .filter_map(|s| s.get("label"))
        .filter_map(|l| match l {
            Value::String(s) => Some(s.clone()),
            Value::Array(a) => a.iter().find_map(|x| x.as_str().map(String::from)),
            _ => None,
        })
        .max_by_key(|s| s.len());

    Ok(Json(serde_json::json!({ "summary": best })))
}

async fn xsearch(state: &AppState, query: &str) -> Result<Value, Response> {
    let response = state
        .http
        .get("https://libris.kb.se/xsearch")
        .query(&[("query", query), ("format", "json"), ("n", "15")])
        .send()
        .await
        .map_err(|e| upstream_error(e.into()))?;
    if !response.status().is_success() {
        return Err(upstream_error(anyhow::anyhow!("status {}", response.status())));
    }
    response.json().await.map_err(|e| upstream_error(e.into()))
}

pub async fn search(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResult>, Response> {
    ensure_enabled(&state).await?;
    let isbn = params
        .isbn
        .as_deref()
        .map(|s| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>())
        .filter(|s| !s.is_empty());
    let title = params.title.as_deref().map(sanitize).filter(|s| !s.is_empty());
    let author = params.author.as_deref().map(sanitize).filter(|s| !s.is_empty());

    let title_query = {
        let mut parts = Vec::new();
        if let Some(t) = &title {
            parts.push(format!("tit:({t})"));
        }
        if let Some(a) = &author {
            parts.push(format!("forf:({a})"));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    };
    let isbn_query = isbn.map(|isbn| format!("isbn:({isbn})"));
    if isbn_query.is_none() && title_query.is_none() {
        return Err(bad_request("need isbn, title or author"));
    }

    let mut body = match &isbn_query {
        Some(q) => xsearch(&state, q).await?,
        None => Value::Null,
    };
    // An ISBN miss (e.g. a foreign edition) falls back to title/author.
    if body["xsearch"]["records"].as_i64().unwrap_or(0) == 0 {
        if let Some(q) = &title_query {
            body = xsearch(&state, q).await?;
        }
    }

    let xsearch = &body["xsearch"];
    let records = xsearch["records"].as_i64().unwrap_or(0);
    let candidates = xsearch["list"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|rec| {
                    let url = first_str(rec.get("identifier"));
                    let libris_id = url
                        .as_deref()
                        .and_then(|u| u.rsplit('/').next())
                        .map(String::from);
                    Candidate {
                        libris_id,
                        url,
                        title: first_str(rec.get("title")),
                        creator: first_str(rec.get("creator")),
                        publisher: first_str(rec.get("publisher")),
                        date: first_str(rec.get("date")),
                        language: first_str(rec.get("language")),
                        isbn: all_str(rec.get("isbn")),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(SearchResult { records, candidates }))
}
