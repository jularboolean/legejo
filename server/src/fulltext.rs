//! Search inside the books: the text of each EPUB in `book_text`, indexed
//! by the database (FTS5 or a GIN index on a tsvector), when the operator
//! has set LEGEJO_FULLTEXT=true. A worker reads the books in the
//! background; started without the variable, it drops the text again.

use crate::auth::AuthUser;
use crate::books::{book_path, Book, BOOK_COLUMNS_B};
use crate::public::owned_expr;
use crate::search::PublicHit;
use crate::booktext::{self, floor_boundary};
use crate::db::Backend;
use crate::formats::Format;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// A section's text is stored in parts of at most this many bytes.
const PART_BYTES: usize = 16_000;
/// Parts looked at per search; the books are ranked by their share of them.
const HIT_LIMIT: i64 = 500;
const BOOK_LIMIT: usize = 50;
/// Passages shown per book.
const PASSAGES: usize = 3;
/// How long the worker sleeps when there is nothing to read.
const IDLE: Duration = Duration::from_secs(10);
/// How often texts no book points to are looked for.
const SWEEP: Duration = Duration::from_secs(300);
/// The marks around a matched word in a passage (private use characters,
/// which the indexed text never holds).
const MARK_START: char = '\u{E000}';
const MARK_END: char = '\u{E001}';

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

/// Whether the operator has turned the search inside the books on.
pub fn enabled(state: &AppState) -> bool {
    state.settings.fulltext
}

/// How far the index has come, for the admin page.
#[derive(Serialize)]
pub struct Status {
    /// EPUBs whose text is in the index, or found to have none.
    pub books: i64,
    /// EPUBs not read yet.
    pub pending: i64,
    /// Distinct texts kept, and their size in bytes (without the index).
    pub texts: i64,
    pub bytes: i64,
}

pub async fn status(state: &AppState) -> anyhow::Result<Status> {
    let (books, pending): (i64, i64) = sqlx::query_as(
        "SELECT
             COALESCE(SUM(CASE WHEN text_sha256 = COALESCE(file_sha256, '') THEN 1 ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN text_sha256 IS NULL OR text_sha256 != COALESCE(file_sha256, '') THEN 1 ELSE 0 END), 0)
         FROM books WHERE format = 'epub'",
    )
    .fetch_one(&state.db)
    .await?;
    let octets = match state.backend {
        Backend::Sqlite => "LENGTH(CAST(body AS BLOB))",
        Backend::Postgres => "OCTET_LENGTH(body)",
    };
    let (texts, bytes): (i64, i64) =
        sqlx::query_as(&format!("SELECT COUNT(DISTINCT text_hash), COALESCE(SUM(CAST({octets} AS BIGINT)), 0) FROM book_text"))
            .fetch_one(&state.db)
            .await?;
    Ok(Status { books, pending, texts, bytes })
}

// ---------------------------------------------------------------------------
// Indexing.

/// One stored part of a section.
#[derive(Debug, PartialEq)]
pub(crate) struct Part {
    pub section: usize,
    pub pos: usize,
    pub place: f64,
    pub title: Option<String>,
    pub body: String,
}

/// The parts of a book's sections: each cut where a line or a word ends,
/// before `PART_BYTES`. The marks are taken out of the text.
pub(crate) fn parts(sections: &[booktext::Section]) -> Vec<Part> {
    let mut out = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        let text = section.text.replace([MARK_START, MARK_END], "");
        let span = section.end - section.start;
        let mut pos = 0;
        while pos < text.len() {
            let mut end = floor_boundary(&text, pos + PART_BYTES);
            if end < text.len() {
                let window = &text[pos..end];
                if let Some(cut) = window.rfind('\n').or_else(|| window.rfind(char::is_whitespace)).filter(|&c| c > 0) {
                    end = pos + cut + 1;
                }
            }
            let body = text[pos..end].trim();
            if !body.is_empty() {
                out.push(Part {
                    section: index,
                    pos,
                    place: section.start + span * pos as f64 / text.len().max(1) as f64,
                    title: section.title.clone(),
                    body: body.to_string(),
                });
            }
            pos = end;
        }
    }
    out
}

/// The key a text is stored under: a hash of its parts.
pub(crate) fn text_hash(parts: &[Part]) -> String {
    let mut all = Vec::new();
    for part in parts {
        all.extend_from_slice(format!("{}\u{1}{}\u{1}{}\u{1}", part.section, part.pos, part.title.as_deref().unwrap_or("")).as_bytes());
        all.extend_from_slice(part.body.as_bytes());
        all.push(0);
    }
    crate::books::sha256_hex(&all)
}

/// Point a book at a stored text, provided the text is (still) there.
/// Returns whether it was.
async fn link(state: &AppState, id: i64, hash: &str, sha: &str) -> anyhow::Result<bool> {
    let done = sqlx::query(
        "UPDATE books SET text_hash = $1, text_sha256 = $2
         WHERE id = $3 AND EXISTS (SELECT 1 FROM book_text WHERE text_hash = $1)",
    )
    .bind(hash)
    .bind(sha)
    .bind(id)
    .execute(&state.db)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// How a book came to have its text.
#[derive(Debug, PartialEq)]
pub(crate) enum Indexed {
    /// Another book has the same file: its text, without reading.
    SameFile,
    /// The file was read, and its text was stored already.
    SameText,
    /// The file was read and its text stored.
    Stored,
    /// The file could not be read, or holds no text.
    Empty,
}

/// Give one book its text. Each text is stored once: a book whose file
/// another book has (an imported copy) gets that book's text without the
/// file being read, and a file read again (its metadata rewritten) whose
/// text is unchanged writes nothing.
async fn index_book(state: &AppState, id: i64, uuid: &str, sha: &str) -> anyhow::Result<Indexed> {
    if !sha.is_empty() {
        let known: Option<String> = sqlx::query_scalar(
            "SELECT text_hash FROM books
             WHERE file_sha256 = $1 AND text_sha256 = $1 AND text_hash IS NOT NULL AND id != $2 LIMIT 1",
        )
        .bind(sha)
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
        if let Some(hash) = known {
            if link(state, id, &hash, sha).await? {
                return Ok(Indexed::SameFile);
            }
        }
    }

    let path = book_path(state, uuid, Format::Epub);
    let found = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Part>> {
        let bytes = std::fs::read(path)?;
        Ok(parts(&booktext::sections(bytes)?))
    })
    .await?;
    // A file that cannot be read is marked as looked at all the same, with
    // no text, so that it is not tried again until the file changes.
    let found = found.unwrap_or_else(|e| {
        tracing::warn!("full-text index: book {id} could not be read: {e:#}");
        Vec::new()
    });
    if found.is_empty() {
        sqlx::query("UPDATE books SET text_hash = NULL, text_sha256 = $1 WHERE id = $2").bind(sha).bind(id).execute(&state.db).await?;
        return Ok(Indexed::Empty);
    }
    let hash = text_hash(&found);
    if link(state, id, &hash, sha).await? {
        return Ok(Indexed::SameText);
    }
    // In one transaction, so that two processes storing the same text
    // cannot leave it half done: the second to commit fails on the unique
    // key, and the next round links the book to what the first stored.
    let mut tx = state.db.begin().await?;
    for part in found {
        sqlx::query("INSERT INTO book_text (text_hash, section, pos, place, title, body) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(&hash)
            .bind(part.section as i64)
            .bind(part.pos as i64)
            .bind(part.place)
            .bind(part.title)
            .bind(part.body)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE books SET text_hash = $1, text_sha256 = $2 WHERE id = $3").bind(&hash).bind(sha).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Indexed::Stored)
}

/// Give the books whose text is missing or older than their file theirs,
/// one at a time. Returns how each went, by book id.
pub(crate) async fn index_pending(state: &AppState, limit: i64) -> anyhow::Result<Vec<(i64, Indexed)>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, uuid, COALESCE(file_sha256, '') FROM books
         WHERE format = 'epub' AND (text_sha256 IS NULL OR text_sha256 != COALESCE(file_sha256, ''))
         ORDER BY id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let mut done = Vec::new();
    for (id, uuid, sha) in rows {
        match index_book(state, id, &uuid, &sha).await {
            Ok(how) => done.push((id, how)),
            Err(e) => tracing::debug!("full-text index: book {id} skipped: {e:#}"),
        }
    }
    Ok(done)
}

/// Drop the texts no book points to any more (their books deleted, or
/// their files changed). Returns how many parts went.
pub(crate) async fn drop_orphans(state: &AppState) -> anyhow::Result<u64> {
    let gone = sqlx::query(
        "DELETE FROM book_text WHERE NOT EXISTS (SELECT 1 FROM books b WHERE b.text_hash = book_text.text_hash)",
    )
    .execute(&state.db)
    .await?;
    Ok(gone.rows_affected())
}

/// Drop all the text, for when the search is turned off.
pub(crate) async fn clear(state: &AppState) -> anyhow::Result<()> {
    let any: Option<i64> = sqlx::query_scalar("SELECT id FROM book_text LIMIT 1").fetch_optional(&state.db).await?;
    let marked: Option<i64> =
        sqlx::query_scalar("SELECT id FROM books WHERE text_sha256 IS NOT NULL LIMIT 1").fetch_optional(&state.db).await?;
    if any.is_none() && marked.is_none() {
        return Ok(());
    }
    sqlx::query("UPDATE books SET text_hash = NULL, text_sha256 = NULL WHERE text_sha256 IS NOT NULL").execute(&state.db).await?;
    sqlx::query("DELETE FROM book_text").execute(&state.db).await?;
    if state.backend == Backend::Sqlite {
        // Gives the index's pages back.
        let _ = sqlx::query("INSERT INTO book_text_fts (book_text_fts) VALUES ('optimize')").execute(&state.db).await;
    }
    tracing::info!("full-text index: turned off, text dropped");
    Ok(())
}

pub async fn worker(state: AppState) {
    let mut last_sweep: Option<std::time::Instant> = None;
    loop {
        let busy = if enabled(&state) {
            match index_pending(&state, 20).await {
                Ok(done) => !done.is_empty(),
                Err(e) => {
                    tracing::warn!("full-text index: {e:#}");
                    false
                }
            }
        } else {
            if let Err(e) = clear(&state).await {
                tracing::warn!("full-text index: {e:#}");
            }
            false
        };
        if !busy {
            if last_sweep.is_none_or(|at| at.elapsed() >= SWEEP) {
                last_sweep = Some(std::time::Instant::now());
                match drop_orphans(&state).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("full-text index: {n} parts of texts no book has any more dropped"),
                    Err(e) => tracing::warn!("full-text index: {e:#}"),
                }
            }
            tokio::time::sleep(IDLE).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Searching.

/// One thing to look for: a word (matched as a prefix) or a phrase (its
/// words in a row, matched whole).
#[derive(Debug, PartialEq)]
pub(crate) enum Term {
    Word(String),
    Phrase(Vec<String>),
}

/// The words of `text` as the index sees them: runs of letters and digits.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// What the user typed, as terms: text in double quotes is a phrase (an
/// unclosed quote runs to the end), other words stand alone. A word the
/// index splits, such as "to-night", is a phrase of its parts.
pub(crate) fn terms(input: &str) -> Vec<Term> {
    let mut out = Vec::new();
    for (i, chunk) in input.split('"').enumerate() {
        if i % 2 == 1 {
            let parts = words(chunk);
            if !parts.is_empty() {
                out.push(Term::Phrase(parts));
            }
            continue;
        }
        for token in chunk.split_whitespace() {
            let mut parts = words(token);
            match parts.len() {
                0 => {}
                1 => out.push(Term::Word(parts.remove(0))),
                _ => out.push(Term::Phrase(parts)),
            }
        }
    }
    out
}

/// The terms as the backend's full-text query; every term must match.
pub(crate) fn text_expr(backend: Backend, input: &str) -> Option<String> {
    let terms = terms(input);
    if terms.is_empty() {
        return None;
    }
    let parts: Vec<String> = terms
        .into_iter()
        .map(|term| match (backend, term) {
            (Backend::Sqlite, Term::Word(w)) => format!("\"{w}\"*"),
            (Backend::Sqlite, Term::Phrase(p)) => format!("\"{}\"", p.join(" ")),
            (Backend::Postgres, Term::Word(w)) => format!("{w}:*"),
            (Backend::Postgres, Term::Phrase(p)) => format!("({})", p.join(" <-> ")),
        })
        .collect();
    Some(parts.join(match backend {
        Backend::Sqlite => " ",
        Backend::Postgres => " & ",
    }))
}

#[derive(Deserialize)]
pub struct Params {
    q: Option<String>,
}

impl Params {
    pub fn new(q: &str) -> Self {
        Params { q: Some(q.to_string()) }
    }
}

#[derive(Serialize, Clone)]
pub struct Passage {
    pub section: i64,
    pub title: Option<String>,
    /// Where in the book, 0–100.
    pub percent: f64,
    /// A stretch of text around the match, the matched words between
    /// U+E000 and U+E001.
    pub text: String,
}

#[derive(Serialize)]
pub struct Hits<B> {
    pub book: B,
    /// Parts of the book that match (within the parts looked at).
    pub hits: usize,
    pub passages: Vec<Passage>,
}

#[derive(Serialize)]
pub struct TextResult {
    /// The caller's own books.
    pub books: Vec<Hits<Book>>,
    /// Books on shelves other users share with the caller, which the
    /// caller has no copy of.
    pub shared: Vec<Hits<PublicHit>>,
    /// The caller's books still being read into the index; their text is
    /// not searched yet.
    pub pending: i64,
    /// More parts matched than were looked at.
    pub truncated: bool,
}

/// Which books a search looks in.
#[derive(Clone, Copy)]
enum Scope {
    Own,
    Shared,
}

/// SQL condition on the book `b` for the scope; $1 is the caller.
fn scope_condition(scope: Scope) -> String {
    match scope {
        Scope::Own => "b.owner_id = $1".to_string(),
        Scope::Shared => format!(
            "b.owner_id != $1 AND {owned} = 0 AND EXISTS (
                 SELECT 1 FROM shelf_books sb JOIN shelves s ON s.id = sb.shelf_id
                 WHERE sb.book_id = b.id AND {visible})",
            owned = owned_expr("$1"),
            visible = crate::shelves::visible_to("$1"),
        ),
    }
}

/// The books in scope whose text matches, most matches first, each with
/// the matching parts in reading order; and whether more parts matched
/// than were looked at.
async fn matching(state: &AppState, user: i64, expr: &str, scope: Scope) -> Result<(Vec<(i64, Vec<i64>)>, bool), Response> {
    let sql = match state.backend {
        Backend::Sqlite => format!(
            "SELECT t.id, b.id FROM book_text_fts f
             JOIN book_text t ON t.id = f.rowid
             JOIN books b ON b.text_hash = t.text_hash
             WHERE book_text_fts MATCH $2 AND {condition}
             ORDER BY b.id, t.section, t.pos LIMIT $3",
            condition = scope_condition(scope),
        ),
        Backend::Postgres => format!(
            "SELECT t.id, b.id FROM book_text t
             JOIN books b ON b.text_hash = t.text_hash
             WHERE to_tsvector('simple', t.body) @@ to_tsquery('simple', $2) AND {condition}
             ORDER BY b.id, t.section, t.pos LIMIT $3",
            condition = scope_condition(scope),
        ),
    };
    let rows: Vec<(i64, i64)> =
        sqlx::query_as(&sql).bind(user).bind(expr).bind(HIT_LIMIT + 1).fetch_all(&state.db).await.map_err(internal)?;
    let truncated = rows.len() as i64 > HIT_LIMIT;
    let mut by_book: Vec<(i64, Vec<i64>)> = Vec::new();
    for (part, book) in rows.into_iter().take(HIT_LIMIT as usize) {
        match by_book.iter_mut().find(|(b, _)| *b == book) {
            Some((_, parts)) => parts.push(part),
            None => by_book.push((book, vec![part])),
        }
    }
    // Most matches first; a stable sort keeps the book order otherwise.
    by_book.sort_by_key(|(_, parts)| std::cmp::Reverse(parts.len()));
    by_book.truncate(BOOK_LIMIT);
    Ok((by_book, truncated))
}

/// The first few matching parts of each book as passages, by part id.
async fn passages(state: &AppState, expr: &str, by_book: &[(i64, Vec<i64>)]) -> Result<HashMap<i64, Passage>, Response> {
    let shown: Vec<String> = by_book.iter().flat_map(|(_, parts)| parts.iter().take(PASSAGES).map(i64::to_string)).collect();
    if shown.is_empty() {
        return Ok(HashMap::new());
    }
    let list = shown.join(",");
    let sql = match state.backend {
        Backend::Sqlite => format!(
            "SELECT t.id, t.section, t.title, t.place,
                    snippet(book_text_fts, 0, char(57344), char(57345), '…', 24) AS text
             FROM book_text_fts f JOIN book_text t ON t.id = f.rowid
             WHERE book_text_fts MATCH $1 AND t.id IN ({list})"
        ),
        Backend::Postgres => format!(
            "SELECT t.id, t.section, t.title, t.place,
                    ts_headline('simple', t.body, to_tsquery('simple', $1),
                        'StartSel=' || chr(57344) || ', StopSel=' || chr(57345) ||
                        ', MaxWords=30, MinWords=15, MaxFragments=1, FragmentDelimiter=\" … \"') AS text
             FROM book_text t WHERE t.id IN ({list})"
        ),
    };
    let rows: Vec<(i64, i64, Option<String>, f64, String)> =
        sqlx::query_as(&sql).bind(expr).fetch_all(&state.db).await.map_err(internal)?;
    Ok(rows
        .into_iter()
        .map(|(part, section, title, place, text)| {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            (part, Passage { section, title, percent: (place * 1000.0).round() / 10.0, text })
        })
        .collect())
}

/// The books in the order of `by_book`, with their passages.
fn assemble<B>(by_book: Vec<(i64, Vec<i64>)>, mut books: HashMap<i64, B>, passages: &HashMap<i64, Passage>) -> Vec<Hits<B>> {
    by_book
        .into_iter()
        .filter_map(|(book_id, parts)| {
            let book = books.remove(&book_id)?;
            // Books with the same text share parts, so passages are copied.
            let shown = parts.iter().take(PASSAGES).filter_map(|p| passages.get(p).cloned()).collect();
            Some(Hits { book, hits: parts.len(), passages: shown })
        })
        .collect()
}

fn id_list(by_book: &[(i64, Vec<i64>)]) -> String {
    by_book.iter().map(|(b, _)| b.to_string()).collect::<Vec<_>>().join(",")
}

/// GET /api/search/text?q= — the books whose text matches: the caller's
/// own, and those on shelves shared with the caller.
pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<Params>,
) -> Result<Json<TextResult>, Response> {
    if !enabled(&state) {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response());
    }
    let id = user.0.id;
    // The caller's books whose file another book has had read (a copy just
    // imported) get that text now rather than when the worker comes by.
    sqlx::query(
        "UPDATE books SET
             text_hash = (SELECT o.text_hash FROM books o
                          WHERE o.file_sha256 = books.file_sha256 AND o.text_sha256 = o.file_sha256
                            AND o.text_hash IS NOT NULL LIMIT 1),
             text_sha256 = file_sha256
         WHERE owner_id = $1 AND format = 'epub' AND file_sha256 IS NOT NULL
           AND (text_sha256 IS NULL OR text_sha256 != file_sha256)
           AND EXISTS (SELECT 1 FROM books o
                       WHERE o.file_sha256 = books.file_sha256 AND o.text_sha256 = o.file_sha256
                         AND o.text_hash IS NOT NULL)",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(internal)?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM books WHERE owner_id = $1 AND format = 'epub'
         AND (text_sha256 IS NULL OR text_sha256 != COALESCE(file_sha256, ''))",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(internal)?;
    let raw = params.q.as_deref().unwrap_or("").trim();
    let Some(expr) = text_expr(state.backend, raw) else {
        return Ok(Json(TextResult { books: Vec::new(), shared: Vec::new(), pending, truncated: false }));
    };

    let (own, own_truncated) = matching(&state, id, &expr, Scope::Own).await?;
    let (shared, shared_truncated) = matching(&state, id, &expr, Scope::Shared).await?;
    let mut found = passages(&state, &expr, &own).await?;
    found.extend(passages(&state, &expr, &shared).await?);

    let own_books: Vec<Book> = if own.is_empty() {
        Vec::new()
    } else {
        // With the reading progress, as the library lists them.
        sqlx::query_as(&format!(
            "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b {joins}
             WHERE b.owner_id = $1 AND b.id IN ({ids})",
            percent = crate::progress::progress_percent(state.backend),
            joins = crate::progress::PROGRESS_JOINS,
            ids = id_list(&own),
        ))
            .bind(id)
            .fetch_all(&state.db)
            .await
            .map_err(internal)?
    };
    // Each shared book with one of the shelves it is shared on, for the link.
    let shared_books: Vec<PublicHit> = if shared.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(&format!(
            "SELECT {BOOK_COLUMNS_B}, u.username AS owner, ps.name AS shelf_name, {owned} AS owned
             FROM books b
             JOIN (
                 SELECT sb.book_id, MIN(s.id) AS shelf_id
                 FROM shelf_books sb
                 JOIN shelves s ON s.id = sb.shelf_id
                 WHERE {visible} AND s.owner_id != $1
                 GROUP BY sb.book_id
             ) pick ON pick.book_id = b.id
             JOIN shelves ps ON ps.id = pick.shelf_id
             JOIN users u ON u.id = ps.owner_id
             WHERE b.id IN ({ids})",
            owned = owned_expr("$1"),
            visible = crate::shelves::visible_to("$1"),
            ids = id_list(&shared),
        ))
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(internal)?
    };

    let books = assemble(own, own_books.into_iter().map(|b| (b.id, b)).collect(), &found);
    let shared = assemble(shared, shared_books.into_iter().map(|h| (h.book.id, h)).collect(), &found);
    Ok(Json(TextResult { books, shared, pending, truncated: own_truncated || shared_truncated }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(text: &str, start: f64, end: f64) -> booktext::Section {
        booktext::Section { title: Some("Ett".into()), text: text.into(), start, end }
    }

    #[test]
    fn a_short_section_is_one_part() {
        let p = parts(&[section("Det var en gång.", 0.0, 0.5), section("Slut.", 0.5, 1.0)]);
        assert_eq!(p.len(), 2);
        assert_eq!((p[1].section, p[1].pos, p[1].place), (1, 0, 0.5));
    }

    #[test]
    fn a_long_section_is_cut_between_words() {
        let text = "ordet ".repeat(PART_BYTES / 3);
        let p = parts(&[section(&text, 0.0, 1.0)]);
        assert!(p.len() > 1);
        assert!(p.iter().all(|part| part.body.len() <= PART_BYTES && part.body.split(' ').all(|w| w == "ordet")));
        assert!(p[1].place > 0.0 && p[1].place < 1.0);
    }

    #[test]
    fn multibyte_text_is_cut_on_char_boundaries() {
        let text = "åäö".repeat(PART_BYTES);
        let p = parts(&[section(&text, 0.0, 1.0)]);
        assert_eq!(p.iter().map(|part| part.body.chars().count()).sum::<usize>(), 3 * PART_BYTES);
    }

    #[test]
    fn quotes_make_a_phrase_and_so_does_a_split_word() {
        use Term::*;
        let w = |s: &str| s.to_string();
        assert_eq!(terms("röda \"lilla stugan\" to-night"), vec![
            Word(w("röda")),
            Phrase(vec![w("lilla"), w("stugan")]),
            Phrase(vec![w("to"), w("night")]),
        ]);
        assert_eq!(terms("\"open ended"), vec![Phrase(vec![w("open"), w("ended")])]);
        assert_eq!(terms("\" \" * ( )"), vec![]);
        assert_eq!(text_expr(Backend::Sqlite, "röd \"lilla stugan\"").unwrap(), "\"röd\"* \"lilla stugan\"");
        assert_eq!(text_expr(Backend::Postgres, "röd \"lilla stugan\"").unwrap(), "röd:* & (lilla <-> stugan)");
        assert!(text_expr(Backend::Sqlite, "\"\"").is_none());
    }

    #[test]
    fn the_marks_are_taken_out() {
        let p = parts(&[section("a\u{E000}b\u{E001}c", 0.0, 1.0)]);
        assert_eq!(p[0].body, "abc");
    }
}
