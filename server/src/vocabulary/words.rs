//! The words page: the reader's books in a language with how much of each
//! they know, the words to learn next, the known words a page at a time,
//! and a search among all the words of their books. Everything is read
//! from a summary per reader and language (`word_sums`, one row per word
//! summed over the books; `book_known`, the share of each book), built from
//! `vocabulary_words` when the page is read and the reader's books have
//! changed since (`summary`), never by the worker. A single tick moves the
//! book shares along; a bulk tick marks them to be built again.

use super::{bad_request, internal, Glossary, QUERY_MAX};
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Any, Transaction};
use std::collections::HashMap;

#[derive(Serialize)]
pub struct WordLanguage {
    pub language: String,
    /// Books of the reader's with a study list in the language.
    pub books: i64,
    pub known: i64,
}

/// GET /api/vocabulary/languages: the languages the reader has study lists
/// or known words in.
pub async fn languages(State(state): State<AppState>, user: AuthUser) -> Response {
    let books: Result<Vec<(String, i64)>, _> = sqlx::query_as(
        "SELECT b.language, COUNT(*) FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         WHERE b.owner_id = $1 AND v.study IS NOT NULL AND v.words > 0
         GROUP BY b.language",
    )
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await;
    let known: Result<Vec<(String, i64)>, _> =
        sqlx::query_as("SELECT language, COUNT(*) FROM known_words WHERE user_id = $1 GROUP BY language")
            .bind(user.0.id)
            .fetch_all(&state.db)
            .await;
    let (Ok(books), Ok(known)) = (books, known) else {
        return internal("languages");
    };
    let mut out: Vec<WordLanguage> = books.into_iter().map(|(language, books)| WordLanguage { language, books, known: 0 }).collect();
    for (language, n) in known {
        match out.iter_mut().find(|l| l.language == language) {
            Some(l) => l.known = n,
            None => out.push(WordLanguage { language, books: 0, known: n }),
        }
    }
    out.sort_by_key(|l| std::cmp::Reverse(l.books + l.known));
    Json(out).into_response()
}

// ---------------------------------------------------------------- summary

/// Bring the reader's summary in a language up to date with their books:
/// built again when a book with a study list came, went or was counted
/// again since (the number of indexed books and the latest indexing among
/// them tell), the book shares alone when the known words changed in bulk
/// or behind the summary's back. Called before the summary is read. The
/// first visit after a change pays for the build; nothing else does.
pub(super) async fn summary(state: &AppState, user: i64, language: &str) -> anyhow::Result<()> {
    let (books_now, indexed_now): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*), MAX(i.computed_at) FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         JOIN vocabulary_indexed i ON i.file_sha256 = b.file_sha256 AND i.language = b.language
         WHERE b.owner_id = $1 AND b.language = $2 AND v.study IS NOT NULL AND v.words > 0",
    )
    .bind(user)
    .bind(language)
    .fetch_one(&state.db)
    .await?;
    let indexed_now = indexed_now.unwrap_or_default();
    let (known_now,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM known_words WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(language)
        .fetch_one(&state.db)
        .await?;
    let built: Option<(i64, String, i64, i64)> =
        sqlx::query_as("SELECT books, indexed_at, known_dirty, known FROM word_summaries WHERE user_id = $1 AND language = $2")
            .bind(user)
            .bind(language)
            .fetch_optional(&state.db)
            .await?;
    match built {
        Some((books, indexed_at, dirty, known)) if books == books_now && indexed_at == indexed_now => {
            if dirty != 0 || known != known_now {
                let mut tx = state.db.begin().await?;
                rebuild_shares(&mut tx, user, language, known_now).await?;
                tx.commit().await?;
            }
        }
        _ => {
            let started = std::time::Instant::now();
            let mut tx = state.db.begin().await?;
            sqlx::query("DELETE FROM word_sums WHERE user_id = $1 AND language = $2").bind(user).bind(language).execute(&mut *tx).await?;
            // The books first (CROSS JOIN keeps that order where the planner
            // takes it literally), each book's words by their key; a name is
            // not a word to learn. `forms` is one book's: a portable string
            // aggregate does not exist.
            sqlx::query(
                "INSERT INTO word_sums (user_id, language, stem, word, forms, total, books)
                 SELECT b.owner_id, b.language, w.stem, MIN(w.word), MIN(w.forms), CAST(SUM(w.n) AS BIGINT), COUNT(*)
                 FROM books b CROSS JOIN vocabulary_words w
                 WHERE b.owner_id = $1 AND b.language = $2
                   AND w.file_sha256 = b.file_sha256 AND w.language = b.language AND w.name = 0
                 GROUP BY b.owner_id, b.language, w.stem",
            )
            .bind(user)
            .bind(language)
            .execute(&mut *tx)
            .await?;
            rebuild_shares(&mut tx, user, language, known_now).await?;
            sqlx::query(
                "INSERT INTO word_summaries (user_id, language, built_at, books, indexed_at, known_dirty, known)
                 VALUES ($1, $2, $3, $4, $5, 0, $6)
                 ON CONFLICT (user_id, language) DO UPDATE SET built_at = excluded.built_at, books = excluded.books,
                     indexed_at = excluded.indexed_at, known_dirty = 0, known = excluded.known",
            )
            .bind(user)
            .bind(language)
            .bind(crate::db::now_ts())
            .bind(books_now)
            .bind(&indexed_now)
            .bind(known_now)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::debug!("vocabulary: words summary of user {user} ({language}) built over {books_now} books in {:?}", started.elapsed());
        }
    }
    Ok(())
}

/// The share of each of the reader's books in a language, from scratch: the
/// words there are to learn (names aside) and how many of them are known.
/// `known_now` is how many words the reader knows in the language, kept
/// with the summary so that a change made elsewhere is noticed.
async fn rebuild_shares(tx: &mut Transaction<'_, Any>, user: i64, language: &str, known_now: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM book_known WHERE user_id = $1 AND book_id IN (SELECT id FROM books WHERE owner_id = $1 AND language = $2)")
        .bind(user)
        .bind(language)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO book_known (user_id, book_id, learnable, known_n)
         SELECT b.owner_id, b.id,
             v.words - CAST(COALESCE(SUM(CASE WHEN w.name = 1 THEN w.n ELSE 0 END), 0) AS BIGINT),
             CAST(COALESCE(SUM(CASE WHEN w.name = 0 AND k.stem IS NOT NULL THEN w.n ELSE 0 END), 0) AS BIGINT)
         FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         JOIN vocabulary_indexed i ON i.file_sha256 = b.file_sha256 AND i.language = b.language
         LEFT JOIN vocabulary_words w ON w.file_sha256 = b.file_sha256 AND w.language = b.language
         LEFT JOIN known_words k ON k.user_id = $1 AND k.language = w.language AND k.stem = w.stem
         WHERE b.owner_id = $1 AND b.language = $2 AND v.study IS NOT NULL AND v.words > 0
         GROUP BY b.owner_id, b.id, v.words",
    )
    .bind(user)
    .bind(language)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE word_summaries SET known_dirty = 0, known = $3 WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(language)
        .bind(known_now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// One word ticked (`known`) or unticked: the share of each of the reader's
/// books that has it moves by the book's uses of it, within the caller's
/// transaction, the one that changed known_words.
pub(super) async fn known_changed(tx: &mut Transaction<'_, Any>, user: i64, language: &str, stem: &str, known: bool) -> anyhow::Result<()> {
    let sign = if known { "+" } else { "-" };
    sqlx::query(&format!(
        "UPDATE book_known SET known_n = known_n {sign} COALESCE(
             (SELECT w.n FROM vocabulary_words w
              JOIN books b ON b.file_sha256 = w.file_sha256 AND b.language = w.language
              WHERE b.id = book_known.book_id AND w.language = $2 AND w.stem = $3 AND w.name = 0), 0)
         WHERE user_id = $1 AND book_id IN
             (SELECT b.id FROM books b
              JOIN vocabulary_words w ON w.file_sha256 = b.file_sha256 AND w.language = b.language
              WHERE b.owner_id = $1 AND w.language = $2 AND w.stem = $3 AND w.name = 0)"
    ))
    .bind(user)
    .bind(language)
    .bind(stem)
    .execute(&mut **tx)
    .await?;
    sqlx::query(&format!("UPDATE word_summaries SET known = known {sign} 1 WHERE user_id = $1 AND language = $2"))
        .bind(user)
        .bind(language)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Many words ticked or unticked at once: the book shares are built again
/// when the page is next read.
pub(super) async fn known_changed_many(tx: &mut Transaction<'_, Any>, user: i64, language: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE word_summaries SET known_dirty = 1 WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(language)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// ------------------------------------------------------------------ rows

#[derive(Serialize, Clone)]
pub struct BookShare {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    /// Share of the book's words (names aside) the reader knows.
    pub share: f64,
    /// Begun and not finished.
    pub reading: bool,
}

#[derive(Serialize, Clone)]
pub struct WordRow {
    pub stem: String,
    pub word: String,
    pub english: Option<String>,
    pub class: Option<&'static str>,
    pub known: bool,
    /// How many of the reader's books it occurs in, and up to `BOOK_IDS` of
    /// them (those that use it most); how often in all.
    pub books: i64,
    pub book_ids: Vec<i64>,
    pub count: u32,
}

/// A word summed over the reader's books: (stem, word, uses, books).
pub(super) type WordSum = (String, String, i64, i64);

impl WordRow {
    pub(super) fn from_sum((stem, word, n, books): WordSum, known: bool) -> WordRow {
        WordRow { stem, word, english: None, class: None, known, books, book_ids: Vec::new(), count: n.clamp(0, u32::MAX as i64) as u32 }
    }

    fn fill_meaning(&mut self, glossary: &Glossary) {
        let m = glossary.meaning(&self.word, &self.stem);
        self.english = m.english;
        self.class = m.class;
    }
}

/// How many of the commonest words in the reader's books they know.
#[derive(Serialize)]
pub struct CommonKnown {
    pub known: i64,
    pub of: i64,
}

/// The commonest words a summary on the words page counts against.
const COMMON_WORDS: i64 = 1000;
/// Books the words page shows at first.
const FIRST_BOOKS: usize = 10;
/// Books a word is said to be in, at most.
const BOOK_IDS: i64 = 20;
/// Words a search gives at most.
const MATCHES: usize = 50;
/// Glossary entries whose English matches a search, at most.
const GLOSS_KEYS: usize = 200;

#[derive(Serialize)]
pub struct Overview {
    pub language: String,
    /// The first of the reader's books in the language, the ones they are
    /// reading first, then the ones they know most of; the rest come from
    /// `books`.
    pub books: Vec<BookShare>,
    pub books_total: usize,
    /// Words to learn next: the commonest across the reader's books that the
    /// reader does not know yet.
    pub next: Vec<WordRow>,
    /// How many words the reader knows; the words themselves come a page at
    /// a time from `known_words`.
    pub known: i64,
    pub common: CommonKnown,
    pub source: Option<String>,
    /// The language pack the meanings come from (none for Esperanto).
    pub pack: Option<crate::langpack::Meta>,
}

/// The reader's word sums in a language ($1 the reader, $2 the language).
const SUMS: &str = "FROM word_sums s WHERE s.user_id = $1 AND s.language = $2";
const IS_KNOWN: &str = "EXISTS (SELECT 1 FROM known_words k WHERE k.user_id = $1 AND k.language = $2 AND k.stem = s.stem)";

/// The reader's books each word occurs in, those that use it most first,
/// up to `BOOK_IDS`: one short query per word, for a few words.
async fn book_ids_of(state: &AppState, user: i64, language: &str, rows: &mut [WordRow]) -> anyhow::Result<()> {
    for r in rows.iter_mut() {
        let ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT b.id FROM vocabulary_words w CROSS JOIN books b
             WHERE w.language = $2 AND w.stem = $3 AND w.name = 0
               AND b.file_sha256 = w.file_sha256 AND b.language = w.language AND b.owner_id = $1
             ORDER BY w.n DESC, b.id LIMIT $4",
        )
        .bind(user)
        .bind(language)
        .bind(&r.stem)
        .bind(BOOK_IDS)
        .fetch_all(&state.db)
        .await?;
        r.book_ids = ids.into_iter().map(|(id,)| id).collect();
    }
    Ok(())
}

/// The reader's books in a language with how much of each they know: those
/// they are reading first (last read first), then those they know most of.
async fn book_shares(state: &AppState, user: i64, language: &str) -> anyhow::Result<Vec<BookShare>> {
    let rows: Vec<(i64, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT b.id, b.title, b.author, s.learnable, s.known_n FROM book_known s
         JOIN books b ON b.id = s.book_id
         WHERE s.user_id = $1 AND b.language = $2",
    )
    .bind(user)
    .bind(language)
    .fetch_all(&state.db)
    .await?;
    let percent = crate::progress::progress_percent(state.backend);
    let last_read = crate::progress::last_read_at(state.backend);
    let joins = crate::progress::PROGRESS_JOINS;
    let reading: Vec<(i64, Option<String>)> = sqlx::query_as(&format!(
        "SELECT b.id, {last_read} FROM books b {joins}
         WHERE b.owner_id = $1 AND b.language = $2 AND {percent} > 0 AND {percent} < 0.99"
    ))
    .bind(user)
    .bind(language)
    .fetch_all(&state.db)
    .await?;
    let reading: HashMap<i64, String> = reading.into_iter().map(|(id, at)| (id, at.unwrap_or_default())).collect();
    let mut books: Vec<BookShare> = rows
        .into_iter()
        .map(|(id, title, author, learnable, known)| BookShare {
            id,
            title,
            author,
            share: known as f64 / learnable.max(1) as f64,
            reading: reading.contains_key(&id),
        })
        .collect();
    books.sort_by(|a, b| {
        b.reading
            .cmp(&a.reading)
            .then_with(|| reading.get(&b.id).cmp(&reading.get(&a.id)))
            .then_with(|| b.share.total_cmp(&a.share))
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(books)
}

/// GET /api/vocabulary/overview?language=: the first of the reader's books
/// in the language with how much of each they know, the words to learn
/// next, and how many words they know.
#[derive(Deserialize)]
pub struct OverviewQuery {
    language: String,
}

pub async fn overview(State(state): State<AppState>, user: AuthUser, Query(q): Query<OverviewQuery>) -> Response {
    match overview_of(&state, user.0.id, q.language).await {
        Ok(o) => Json(o).into_response(),
        Err(e) => internal(e),
    }
}

async fn overview_of(state: &AppState, user: i64, language: String) -> anyhow::Result<Overview> {
    summary(state, user, &language).await?;
    let glossary = Glossary::for_language(state, &language);
    let mut books = book_shares(state, user, &language).await?;
    let books_total = books.len();
    books.truncate(FIRST_BOOKS);
    // Words to learn next: in more books first, then more often.
    let next: Vec<WordSum> = sqlx::query_as(&format!(
        "SELECT s.stem, s.word, s.total, s.books {SUMS} AND NOT {IS_KNOWN}
         ORDER BY s.books DESC, s.total DESC, s.word LIMIT 30"
    ))
    .bind(user)
    .bind(&language)
    .fetch_all(&state.db)
    .await?;
    let mut next: Vec<WordRow> = next.into_iter().map(|t| WordRow::from_sum(t, false)).collect();
    let (known,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM known_words WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(&language)
        .fetch_one(&state.db)
        .await?;
    // How many of the commonest words the reader knows.
    let (common_known, common_of): (i64, i64) = sqlx::query_as(&format!(
        "SELECT COUNT(k.stem), COUNT(*) FROM
           (SELECT s.stem {SUMS} ORDER BY s.total DESC, s.stem LIMIT {COMMON_WORDS}) t
         LEFT JOIN known_words k ON k.user_id = $1 AND k.language = $2 AND k.stem = t.stem"
    ))
    .bind(user)
    .bind(&language)
    .fetch_one(&state.db)
    .await?;
    book_ids_of(state, user, &language, &mut next).await?;
    for row in next.iter_mut() {
        row.fill_meaning(&glossary);
    }
    Ok(Overview {
        language,
        books,
        books_total,
        next,
        known,
        common: CommonKnown { known: common_known, of: common_of },
        source: glossary.attribution(),
        pack: glossary.pack().map(|p| p.meta.clone()),
    })
}

// ---------------------------------------------------------------- search

/// GET /api/vocabulary/search?language=&q=: words of the reader's books in
/// the language that match what they typed, with the known ones among them.
#[derive(Deserialize)]
pub struct SearchQuery {
    language: String,
    q: String,
}

pub async fn search(State(state): State<AppState>, user: AuthUser, Query(q): Query<SearchQuery>) -> Response {
    if q.q.chars().count() > QUERY_MAX {
        return bad_request("a shorter search");
    }
    let found = async {
        summary(&state, user.0.id, &q.language).await?;
        let needle = q.q.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        let glossary = Glossary::for_language(&state, &q.language);
        search_words(&state, user.0.id, &q.language, &glossary, &needle).await
    }
    .await;
    match found {
        Ok(matches) => Json(json!({ "matches": matches })).into_response(),
        Err(e) => internal(e),
    }
}

/// A LIKE pattern that finds `q` anywhere, with its own wildcards escaped
/// (the statements say ESCAPE '\').
fn like_anywhere(q: &str) -> String {
    let mut out = String::from("%");
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Words of the reader's books that match a search (lower case): by the
/// word, one of the forms the books write it in, or its English. Exact
/// words first, then the commonest.
async fn search_words(state: &AppState, user: i64, language: &str, glossary: &Glossary, q: &str) -> anyhow::Result<Vec<WordRow>> {
    type Hit = (String, String, i64, i64, i64);
    let by_form: Vec<Hit> = sqlx::query_as(&format!(
        "SELECT s.stem, s.word, s.total, s.books, CASE WHEN {IS_KNOWN} THEN 1 ELSE 0 END {SUMS}
           AND (s.stem LIKE $3 ESCAPE '\\' OR s.word LIKE $3 ESCAPE '\\' OR s.forms LIKE $3 ESCAPE '\\')
         ORDER BY CASE WHEN s.stem = $4 THEN 0 ELSE 1 END, s.total DESC, s.word LIMIT {MATCHES}"
    ))
    .bind(user)
    .bind(language)
    .bind(like_anywhere(q))
    .bind(q)
    .fetch_all(&state.db)
    .await?;
    let mut found: Vec<WordRow> = by_form.into_iter().map(|(stem, word, n, books, known)| WordRow::from_sum((stem, word, n, books), known == 1)).collect();
    // Some of the reader's words by their keys.
    let by_keys = |keys: Vec<String>| async move {
        if keys.is_empty() {
            return Ok::<Vec<Hit>, anyhow::Error>(Vec::new());
        }
        let marks: Vec<String> = (0..keys.len()).map(|i| format!("${}", i + 3)).collect();
        let sql = format!("SELECT s.stem, s.word, s.total, s.books, CASE WHEN {IS_KNOWN} THEN 1 ELSE 0 END {SUMS} AND s.stem IN ({})", marks.join(", "));
        let mut query = sqlx::query_as::<_, Hit>(&sql).bind(user).bind(language);
        for key in keys {
            query = query.bind(key);
        }
        query.fetch_all(&state.db).await.map_err(anyhow::Error::from)
    };
    // By the glossary's forms: the words the search is a form of ("hizo"
    // is "hacer"), which the few forms kept per word need not show. These
    // count as exact.
    let lemmas: Vec<String> = match glossary {
        Glossary::Pack(p) => p.lemmas_of(q).iter().filter_map(|l| p.lemma(*l).map(|x| x.word.to_string())).collect(),
        Glossary::Espdic => vec![crate::esperanto::stem(&crate::esperanto::supersigned(q).unwrap_or_else(|| q.to_string()))],
        Glossary::None => Vec::new(),
    };
    let by_lemma = by_keys(lemmas.clone()).await?;
    // By the English: the glossary's entries whose meaning holds the search,
    // looked up among the reader's words. ESPDIC has no index by meaning,
    // so there the reader's commonest words are looked up one by one.
    let by_meaning: Vec<Hit> = match glossary {
        Glossary::Pack(p) => by_keys(p.keys_with_gloss(q, GLOSS_KEYS).into_iter().map(String::from).collect()).await?,
        Glossary::Espdic => {
            let rows: Vec<Hit> = sqlx::query_as(&format!(
                "SELECT s.stem, s.word, s.total, s.books, CASE WHEN {IS_KNOWN} THEN 1 ELSE 0 END {SUMS} ORDER BY s.total DESC LIMIT 8000"
            ))
            .bind(user)
            .bind(language)
            .fetch_all(&state.db)
            .await?;
            rows.into_iter().filter(|(stem, word, ..)| glossary.meaning(word, stem).english.is_some_and(|e| e.to_lowercase().contains(q))).collect()
        }
        Glossary::None => Vec::new(),
    };
    for (stem, word, n, books, known) in by_lemma.into_iter().chain(by_meaning) {
        if !found.iter().any(|f| f.stem == stem) {
            found.push(WordRow::from_sum((stem, word, n, books), known == 1));
        }
    }
    found.sort_by(|a, b| {
        let exact = |w: &WordRow| w.stem != q && w.word.to_lowercase() != q && !lemmas.contains(&w.stem);
        (exact(a), b.count).cmp(&(exact(b), a.count))
    });
    found.truncate(MATCHES);
    book_ids_of(state, user, language, &mut found).await?;
    for row in found.iter_mut() {
        row.fill_meaning(glossary);
    }
    Ok(found)
}

// ----------------------------------------------------------------- books

/// GET /api/vocabulary/books?language=&offset=&limit=: the reader's books in
/// a language with how much of each they know, a page at a time, in the
/// order of the overview.
#[derive(Deserialize)]
pub struct BooksQuery {
    language: String,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

pub async fn books(State(state): State<AppState>, user: AuthUser, Query(q): Query<BooksQuery>) -> Response {
    let all = async {
        summary(&state, user.0.id, &q.language).await?;
        book_shares(&state, user.0.id, &q.language).await
    }
    .await;
    match all {
        Ok(all) => {
            let total = all.len();
            let page: Vec<BookShare> = all.into_iter().skip(q.offset).take(q.limit.unwrap_or(100).min(500)).collect();
            Json(json!({ "total": total, "books": page })).into_response()
        }
        Err(e) => internal(e),
    }
}

// ----------------------------------------------------------- known words

/// GET /api/vocabulary/known/words?language=&q=&sort=&class=&offset=&limit=:
/// the words the reader knows, a page at a time, with their meanings and the
/// books they occur in. `sort` is `recent` (the default: last ticked first),
/// `common` (most often in the reader's books) or `alpha`; without `limit`
/// all of them come (for an export), then without the books of each.
#[derive(Deserialize)]
pub struct KnownWordsQuery {
    language: String,
    q: Option<String>,
    sort: Option<String>,
    class: Option<String>,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct KnownWords {
    /// How many known words match the filter and class.
    pub total: usize,
    pub words: Vec<WordRow>,
}

pub async fn known_words(State(state): State<AppState>, user: AuthUser, Query(q): Query<KnownWordsQuery>) -> Response {
    if q.q.as_ref().is_some_and(|s| s.chars().count() > QUERY_MAX) {
        return bad_request("a shorter search");
    }
    match known_words_of(&state, user.0.id, q).await {
        Ok(k) => Json(k).into_response(),
        Err(e) => internal(e),
    }
}

async fn known_words_of(state: &AppState, user: i64, q: KnownWordsQuery) -> anyhow::Result<KnownWords> {
    summary(state, user, &q.language).await?;
    let glossary = Glossary::for_language(state, &q.language);
    let filter = q.q.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let class = q.class.as_deref().filter(|c| !c.is_empty() && *c != "all");
    // A known stem with, when the reader's books have it, its word, uses
    // and books; sorted in the database.
    type Ticked = (String, Option<String>, Option<i64>, Option<i64>);
    let order = match q.sort.as_deref() {
        Some("common") => "COALESCE(s.total, 0) DESC, COALESCE(s.word, k.stem)",
        Some("alpha") => "LOWER(COALESCE(s.word, k.stem)), k.stem",
        _ => "k.created_at DESC, k.stem",
    };
    let select = format!(
        "SELECT k.stem, s.word, s.total, s.books FROM known_words k
         LEFT JOIN word_sums s ON s.user_id = k.user_id AND s.language = k.language AND s.stem = k.stem
         WHERE k.user_id = $1 AND k.language = $2 ORDER BY {order}"
    );
    let row = |(stem, word, n, books): Ticked| {
        let word = word.unwrap_or_else(|| stem.clone());
        WordRow::from_sum((stem, word, n.unwrap_or(0), books.unwrap_or(0)), true)
    };
    let (total, mut page) = if filter.is_none() && class.is_none() {
        // Nothing to look up before the page is cut: the database cuts it.
        let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM known_words WHERE user_id = $1 AND language = $2")
            .bind(user)
            .bind(&q.language)
            .fetch_one(&state.db)
            .await?;
        let ticked: Vec<Ticked> = match q.limit {
            Some(limit) => {
                sqlx::query_as(&format!("{select} LIMIT $3 OFFSET $4"))
                    .bind(user)
                    .bind(&q.language)
                    .bind(limit as i64)
                    .bind(q.offset as i64)
                    .fetch_all(&state.db)
                    .await?
            }
            None => sqlx::query_as(&select).bind(user).bind(&q.language).fetch_all(&state.db).await?,
        };
        let mut page: Vec<WordRow> = ticked.into_iter().map(row).collect();
        for w in page.iter_mut() {
            w.fill_meaning(&glossary);
        }
        (total as usize, page)
    } else {
        // The filter needs every word's meaning: all of them, then the page.
        let ticked: Vec<Ticked> = sqlx::query_as(&select).bind(user).bind(&q.language).fetch_all(&state.db).await?;
        let mut rows: Vec<WordRow> = ticked.into_iter().map(row).collect();
        for w in rows.iter_mut() {
            w.fill_meaning(&glossary);
        }
        rows.retain(|w| {
            filter.as_ref().is_none_or(|f| w.word.to_lowercase().contains(f.as_str()) || w.english.as_ref().is_some_and(|e| e.to_lowercase().contains(f.as_str())))
                && class.is_none_or(|c| w.class.unwrap_or("other") == c)
        });
        let total = rows.len();
        (total, rows.into_iter().skip(q.offset).take(q.limit.unwrap_or(usize::MAX)).collect())
    };
    // The books of each word on a page; an export of every word does without.
    if q.limit.is_some() {
        book_ids_of(state, user, &q.language, &mut page).await?;
    }
    Ok(KnownWords { total, words: page })
}
