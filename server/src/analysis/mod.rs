//! A book's analysis, for the reader who wants to look closer: its style
//! and readability measured, its fixed phrases, the people and places it
//! names, and its grammar. Nothing here is computed until a reader asks for
//! a book's analysis; then it is kept in `book_analysis`, once per file and
//! language, as the vocabulary is. The concordance (every line of the book
//! with a word or a phrase) is read from the text each time it is asked for.
//!
//! - `text`: the text as sentences of numbered words;
//! - `style`: sentence and word length, variation, dialogue, readability;
//! - `phrases`: fixed phrases, and the names with their links;
//! - `grammar`: the forms the book is written in, counted by what they are;
//! - `concordance`: a word or a phrase in its lines.

mod concordance;
mod grammar;
mod phrases;
mod reach;
mod style;
mod text;

pub use concordance::concordance;
pub use grammar::forms;
pub(crate) use grammar::has_forms;

use crate::auth::AuthUser;
use crate::booktext;
use crate::formats::Format;
use crate::langpack::Pack;
use crate::vocabulary::{give_memory_back, pack_for, stopwords, Stemmer};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// Raised when what is computed changes, so that a book is analysed again
/// the next time it is asked for.
const VERSION: i64 = 3;
/// Words a book must have before it is analysed.
const WORDS_MIN: usize = 300;
/// A text whose "words" are this long on average is not written in words
/// set apart by spaces (Chinese, Japanese), and is not analysed.
const WORD_LETTERS_MAX: f64 = 20.0;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn not_found(what: &str) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": what }))).into_response()
}

fn bad_request(what: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": what }))).into_response()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Analysis {
    pub style: style::Style,
    pub phrases: Vec<phrases::Phrase>,
    pub people: phrases::People,
    /// None for a language whose forms are not known by what they are.
    #[serde(default)]
    pub grammar: Option<grammar::Grammar>,
    /// None for a language without a frequency list (a pack's extras).
    #[serde(default)]
    pub reach: Option<reach::Reach>,
}

/// A text's analysis, or None when it is too short or not written in words.
pub(crate) fn compute(text: &str, language: &str, pack: Option<Arc<Pack>>) -> Option<Analysis> {
    let p = text::prepare(text);
    if p.tokens.len() < WORDS_MIN {
        return None;
    }
    let style = style::style(&p, language);
    if style.word_mean > WORD_LETTERS_MAX {
        return None;
    }
    let names = phrases::name_words(&p, language);
    let stemmer = Stemmer::for_book(language, pack.clone());
    Some(Analysis {
        phrases: phrases::phrases(&p, &names),
        people: phrases::people(&p, &names, stemmer.as_ref(), &stopwords(language)),
        grammar: grammar::grammar(&p, language, pack.as_deref(), &names),
        reach: pack.as_deref().and_then(|pack| reach::reach(&p, pack, &names)),
        style,
    })
}

/// What `book_analysis.pack` holds: the id of the pack the book was
/// analysed with, or '' without one.
fn pack_id(pack: Option<&Arc<Pack>>) -> String {
    pack.map(|p| p.id_with_extras()).unwrap_or_default()
}

/// A reader's book as the analysis needs it.
pub(super) struct Book {
    pub uuid: String,
    pub sha: String,
    pub language: String,
    pub text_hash: Option<String>,
}

/// The reader's EPUB with a file and a language, or the answer to give.
pub(super) async fn book(state: &AppState, user: i64, id: i64) -> Result<Book, Response> {
    let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT uuid, format, file_sha256, language, text_hash FROM books WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(user)
            .fetch_optional(&state.db)
            .await
            .map_err(internal)?;
    match row {
        Some((uuid, format, Some(sha), Some(language), text_hash)) if format == "epub" => Ok(Book { uuid, sha, language, text_hash }),
        _ => Err(not_found("no analysis")),
    }
}

/// The book's text from its file, the sections a line break apart.
pub(super) async fn file_text(state: &AppState, uuid: &str) -> anyhow::Result<String> {
    let path = crate::books::book_path(state, uuid, Format::Epub);
    anyhow::ensure!(state.storage.fetch(&path).await, "the file of book {uuid} is not there");
    tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let sections = booktext::read_sections(Format::Epub, &path)?;
        Ok(sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n"))
    })
    .await?
}

/// GET /api/books/{id}/analysis: the book's analysis, computed now when it
/// has not been (or was by another version or with another pack). 404 for a
/// book that is not an EPUB with a language, or is too short.
pub async fn for_book(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Response {
    let book = match book(&state, user.0.id, id).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let pack = pack_for(&state, &book.language);
    let expected = pack_id(pack.as_ref());
    let kept: Option<(String,)> =
        match sqlx::query_as("SELECT data FROM book_analysis WHERE file_sha256 = $1 AND language = $2 AND version = $3 AND pack = $4")
            .bind(&book.sha)
            .bind(&book.language)
            .bind(VERSION)
            .bind(&expected)
            .fetch_optional(&state.db)
            .await
        {
            Ok(k) => k,
            Err(e) => return internal(e),
        };
    let data = match kept {
        Some((data,)) => data,
        None => match analyse(&state, &book, pack, &expected).await {
            Ok(data) => data,
            Err(e) => return internal(e),
        },
    };
    let analysis: Option<Analysis> = match serde_json::from_str(&data) {
        Ok(a) => a,
        Err(e) => return internal(e),
    };
    let Some(analysis) = analysis else { return not_found("no analysis") };
    let mut answer = match serde_json::to_value(&analysis) {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    answer["language"] = json!(book.language);
    answer["forms"] = json!(grammar::has_forms(&state, &book.language));
    Json(answer).into_response()
}

/// Analyse a book and keep the result; "null" for a book with nothing to
/// analyse, so that its file is not read again each time.
async fn analyse(state: &AppState, book: &Book, pack: Option<Arc<Pack>>, pack_id: &str) -> anyhow::Result<String> {
    let text = file_text(state, &book.uuid).await?;
    let language = book.language.clone();
    let analysis = tokio::task::spawn_blocking(move || compute(&text, &language, pack)).await?;
    give_memory_back();
    let data = serde_json::to_string(&analysis)?;
    sqlx::query(
        "INSERT INTO book_analysis (file_sha256, language, version, pack, data, computed_at) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (file_sha256, language) DO UPDATE SET version = excluded.version, pack = excluded.pack,
             data = excluded.data, computed_at = excluded.computed_at",
    )
    .bind(&book.sha)
    .bind(&book.language)
    .bind(VERSION)
    .bind(pack_id)
    .bind(&data)
    .bind(crate::db::now_ts())
    .execute(&state.db)
    .await?;
    Ok(data)
}

/// Drop the analyses no book's file and language point to any more.
/// Returns how many went.
pub(crate) async fn drop_orphans(state: &AppState) -> anyhow::Result<u64> {
    let gone = sqlx::query(
        "DELETE FROM book_analysis WHERE NOT EXISTS
           (SELECT 1 FROM books b WHERE b.file_sha256 = book_analysis.file_sha256 AND b.language = book_analysis.language)",
    )
    .execute(&state.db)
    .await?;
    Ok(gone.rows_affected())
}
