//! OPDS 1.2 catalog for reader apps (KOReader, Moon+ and similar). HTTP Basic
//! auth with the username and either an app password (app_passwords.rs) or
//! the account password.

use crate::books::{search_expr, search_parts, Book, BOOK_COLUMNS, BOOK_COLUMNS_B};
use crate::AppState;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::{Path, Query, State};
use axum::http::{header, request::Parts, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;

pub struct BasicUser {
    pub id: i64,
}

impl axum::extract::FromRequestParts<AppState> for BasicUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let challenge = || {
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"Legejo\"")],
                "authentication required",
            )
                .into_response()
        };

        let value = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Basic "))
            .ok_or_else(challenge)?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(value)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(challenge)?;
        let (username, password) = decoded.split_once(':').ok_or_else(challenge)?;

        let row: Option<(i64, String)> =
            sqlx::query_as("SELECT id, password_hash FROM users WHERE LOWER(username) = LOWER($1)")
                .bind(username.trim())
                .fetch_optional(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
        let (id, hash) = row.ok_or_else(challenge)?;
        // App passwords first: a cheap lookup compared to Argon2.
        if crate::app_passwords::matches(state, id, password).await {
            return Ok(BasicUser { id });
        }
        let parsed = PasswordHash::new(&hash).map_err(|e| internal(anyhow::anyhow!("{e}")))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| challenge())?;
        Ok(BasicUser { id })
    }
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const NAV_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

fn feed_response(kind: &str, xml: String) -> Response {
    ([(header::CONTENT_TYPE, kind.to_string())], xml).into_response()
}

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn feed_open(id: &str, title: &str, self_href: &str, self_type: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog">
<id>{id}</id>
<title>{title}</title>
<updated>{updated}</updated>
<link rel="self" href="{href}" type="{stype}"/>
<link rel="start" href="/api/opds" type="{nav}"/>
<link rel="search" href="/api/opds/search.xml" type="application/opensearchdescription+xml"/>
"#,
        id = esc(id),
        title = esc(title),
        updated = now(),
        href = esc(self_href),
        stype = self_type,
        nav = NAV_TYPE,
    )
}

fn nav_entry(title: &str, content: &str, href: &str, kind: &str) -> String {
    format!(
        r#"<entry>
<title>{title}</title>
<id>{href}</id>
<updated>{updated}</updated>
<content type="text">{content}</content>
<link href="{href}" type="{kind}"/>
</entry>
"#,
        title = esc(title),
        content = esc(content),
        href = esc(href),
        updated = now(),
        kind = kind,
    )
}

fn book_entry(book: &Book) -> String {
    let mut entry = format!(
        r#"<entry>
<title>{title}</title>
<id>urn:uuid:{uuid}</id>
<updated>{updated}</updated>
"#,
        title = esc(&book.title),
        uuid = book.uuid,
        updated = book.updated_at.as_deref().unwrap_or(&book.created_at),
    );
    if let Some(author) = &book.author {
        entry.push_str(&format!("<author><name>{}</name></author>\n", esc(author)));
    }
    if let Some(language) = &book.language {
        entry.push_str(&format!("<dc:language>{}</dc:language>\n", esc(language)));
    }
    if let Some(published) = &book.published {
        entry.push_str(&format!("<dc:issued>{}</dc:issued>\n", esc(published)));
    }
    if let Some(category) = &book.category {
        entry.push_str(&format!("<category term=\"{}\"/>\n", esc(category)));
    }
    // OPDS 1.2 has no series element. Readers show the summary, so the
    // series is prepended to it, as calibre does.
    let series = book.series.as_deref().filter(|s| !s.trim().is_empty()).map(|s| match book.series_index {
        Some(i) => format!("{s} #{}", series_number(i)),
        None => s.to_string(),
    });
    let mut lead: Vec<String> = Vec::new();
    if let Some(s) = &series {
        lead.push(format!("Series: {s}"));
    }
    // Likewise for rating.
    if let Some(rating) = book.rating {
        lead.push(format!("Rating: {rating}/5"));
    }
    let summary = match (lead.is_empty(), &book.description) {
        (false, Some(d)) => Some(format!("{}\n\n{d}", lead.join("\n"))),
        (false, None) => Some(lead.join("\n")),
        (true, Some(d)) => Some(d.clone()),
        (true, None) => None,
    };
    if let Some(summary) = summary {
        entry.push_str(&format!("<summary type=\"text\">{}</summary>\n", esc(&summary)));
    }
    if let Some(name) = book.series.as_deref().filter(|s| !s.trim().is_empty()) {
        entry.push_str(&format!(
            "<link rel=\"related\" href=\"{}\" type=\"{ACQ_TYPE}\" title=\"{}\"/>\n",
            esc(&series_href(name)),
            esc(name)
        ));
    }
    if book.has_cover.as_bool() {
        entry.push_str(&format!(
            "<link rel=\"http://opds-spec.org/image\" href=\"/api/opds/books/{id}/cover\" type=\"image/jpeg\"/>\n",
            id = book.id
        ));
    }
    entry.push_str(&format!(
        "<link rel=\"http://opds-spec.org/acquisition\" href=\"/api/opds/books/{id}/file\" type=\"application/epub+zip\"/>\n</entry>\n",
        id = book.id
    ));
    entry
}

pub async fn root(_user: BasicUser) -> Response {
    let mut xml = feed_open("legejo:root", "Legejo", "/api/opds", NAV_TYPE);
    xml.push_str(&nav_entry("All books", "Every book in the catalog", "/api/opds/books", ACQ_TYPE));
    xml.push_str(&nav_entry("Shelves", "Books by shelf", "/api/opds/shelves", NAV_TYPE));
    xml.push_str(&nav_entry("Series", "Books by series", "/api/opds/series", NAV_TYPE));
    xml.push_str("</feed>\n");
    feed_response(NAV_TYPE, xml)
}

/// "1" for 1.0, "1.5" for 1.5; series indices are floats.
fn series_number(i: f64) -> String {
    if i.fract() == 0.0 { format!("{}", i as i64) } else { format!("{i}") }
}

/// Series have no id; the name goes in the path, percent-encoded.
fn series_href(name: &str) -> String {
    let mut out = String::from("/api/opds/series/");
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub async fn series_list(State(state): State<AppState>, user: BasicUser) -> Result<Response, Response> {
    let series: Vec<(String, i64)> = sqlx::query_as(
        "SELECT MIN(series), COUNT(*) FROM books
         WHERE owner_id = $1 AND series IS NOT NULL AND TRIM(series) <> ''
         GROUP BY LOWER(series)
         ORDER BY LOWER(MIN(series))",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let mut xml = feed_open("legejo:series", "Series", "/api/opds/series", NAV_TYPE);
    for (name, count) in &series {
        xml.push_str(&nav_entry(name, &format!("{count} books"), &series_href(name), ACQ_TYPE));
    }
    xml.push_str("</feed>\n");
    Ok(feed_response(NAV_TYPE, xml))
}

pub async fn series(
    State(state): State<AppState>,
    user: BasicUser,
    Path(name): Path<String>,
) -> Result<Response, Response> {
    let books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b
         WHERE b.owner_id = $1 AND b.series IS NOT NULL AND LOWER(b.series) = LOWER($2)
         ORDER BY CASE WHEN b.series_index IS NULL THEN 1 ELSE 0 END, b.series_index, LOWER(b.title)"
    ))
    .bind(user.id)
    .bind(&name)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if books.is_empty() {
        return Err(not_found());
    }

    let mut xml = feed_open(&format!("legejo:series:{}", name.to_lowercase()), &name, &series_href(&name), ACQ_TYPE);
    for book in &books {
        xml.push_str(&book_entry(book));
    }
    xml.push_str("</feed>\n");
    Ok(feed_response(ACQ_TYPE, xml))
}

pub async fn opensearch() -> Response {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
<ShortName>Legejo</ShortName>
<Description>Search the catalog</Description>
<Url type="application/atom+xml;profile=opds-catalog;kind=acquisition" template="/api/opds/books?q={searchTerms}"/>
</OpenSearchDescription>
"#;
    ([(header::CONTENT_TYPE, "application/opensearchdescription+xml")], xml).into_response()
}

#[derive(serde::Deserialize)]
pub struct BooksParams {
    q: Option<String>,
}

pub async fn books(
    State(state): State<AppState>,
    user: BasicUser,
    Query(params): Query<BooksParams>,
) -> Result<Response, Response> {
    let query = params.q.as_deref().and_then(|q| search_expr(state.backend, q));
    let books: Vec<Book> = match &query {
        Some(match_expr) => {
            let parts = search_parts(state.backend);
            sqlx::query_as(&format!(
                "SELECT {BOOK_COLUMNS_B} FROM books b
                 {join}
                 WHERE b.owner_id = $1 AND {condition}
                 ORDER BY {order}",
                join = parts.join,
                condition = parts.condition,
                order = parts.order,
            ))
            .bind(user.id)
            .bind(match_expr)
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query_as(&format!(
                "SELECT {BOOK_COLUMNS} FROM books WHERE owner_id = $1 ORDER BY created_at DESC, id DESC"
            ))
            .bind(user.id)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| internal(e.into()))?;

    let (title, href) = match params.q.as_deref() {
        Some(q) => (format!("Search: {q}"), format!("/api/opds/books?q={}", esc(q))),
        None => ("All books".to_string(), "/api/opds/books".to_string()),
    };
    let mut xml = feed_open("legejo:books", &title, &href, ACQ_TYPE);
    for book in &books {
        xml.push_str(&book_entry(book));
    }
    xml.push_str("</feed>\n");
    Ok(feed_response(ACQ_TYPE, xml))
}

pub async fn shelves(State(state): State<AppState>, user: BasicUser) -> Result<Response, Response> {
    let shelves: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT s.id, s.name, COUNT(sb.book_id) FROM shelves s
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE s.owner_id = $1
         GROUP BY s.id
         ORDER BY LOWER(s.name)",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let mut xml = feed_open("legejo:shelves", "Shelves", "/api/opds/shelves", NAV_TYPE);
    for (id, name, count) in &shelves {
        xml.push_str(&nav_entry(
            name,
            &format!("{count} books"),
            &format!("/api/opds/shelves/{id}"),
            ACQ_TYPE,
        ));
    }
    xml.push_str("</feed>\n");
    Ok(feed_response(NAV_TYPE, xml))
}

pub async fn shelf(
    State(state): State<AppState>,
    user: BasicUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM shelves WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let name = name.ok_or_else(not_found)?;

    let books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1 AND b.owner_id = $2
         ORDER BY sb.added_at DESC, b.id DESC"
    ))
    .bind(id)
    .bind(user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let href = format!("/api/opds/shelves/{id}");
    let mut xml = feed_open(&format!("legejo:shelf:{id}"), &name, &href, ACQ_TYPE);
    for book in &books {
        xml.push_str(&book_entry(book));
    }
    xml.push_str("</feed>\n");
    Ok(feed_response(ACQ_TYPE, xml))
}

async fn owned_book(
    state: &AppState,
    user_id: i64,
    book_id: i64,
) -> Result<(String, Option<String>, String), Response> {
    let row: Option<(String, Option<String>, String)> =
        sqlx::query_as("SELECT uuid, cover_mime, title FROM books WHERE id = $1 AND owner_id = $2")
            .bind(book_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    row.ok_or_else(not_found)
}

pub async fn cover(
    State(state): State<AppState>,
    user: BasicUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let (uuid, cover_mime, _) = owned_book(&state, user.id, id).await?;
    let mime = cover_mime.ok_or_else(not_found)?;
    let data = tokio::fs::read(state.data_dir.join("covers").join(&uuid))
        .await
        .map_err(|_| not_found())?;
    Ok(([(header::CONTENT_TYPE, mime)], data).into_response())
}

pub async fn download(
    State(state): State<AppState>,
    user: BasicUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let (uuid, _, title) = owned_book(&state, user.id, id).await?;
    let data = tokio::fs::read(state.data_dir.join("books").join(format!("{uuid}.epub")))
        .await
        .map_err(|e| internal(e.into()))?;
    let safe_title: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, "application/epub+zip".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{safe_title}.epub\"")),
        ],
        data,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_numbers_and_hrefs() {
        assert_eq!(series_number(3.0), "3");
        assert_eq!(series_number(1.5), "1.5");
        assert_eq!(series_href("Sagan om ringen"), "/api/opds/series/Sagan%20om%20ringen");
        assert_eq!(series_href("Å/ä"), "/api/opds/series/%C3%85%2F%C3%A4");
    }
}
