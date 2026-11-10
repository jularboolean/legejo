//! The worker: counts the words of each book once per file, language and
//! language pack, keeps the result in `vocabulary`, its study list in
//! `vocabulary_words` and its candidate words in `vocabulary_df`, and drops
//! what no book points to any more.
//!
//! A book's words, its candidates' counts in `vocabulary_df` and its
//! `vocabulary_indexed` row are written in one transaction, so that the
//! candidates of a book are counted in `vocabulary_df` exactly when the book
//! has an indexed row. A book without one is indexed by `index_pending`.

use super::{compute, pack_for, pack_id, Candidate, Computed, StudyWord, VERSION};
use crate::booktext;
use crate::db::Backend;
use crate::formats::Format;
use crate::AppState;
use sqlx::{Any, Executor, Transaction};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(30);
const SWEEP: Duration = Duration::from_secs(600);

async fn store(state: &AppState, sha: &str, language: &str, pack: &str, c: Option<Computed>) -> anyhow::Result<()> {
    let (words, stems, hapax, need, coverage, curve, candidates, study) = match c {
        Some(c) => (
            c.words as i64,
            c.stems as i64,
            c.hapax as i64,
            c.sample_need as i64,
            serde_json::to_string(&c.coverage)?,
            serde_json::to_string(&c.curve)?,
            serde_json::to_string(&c.candidates)?,
            c.study.map(|s| serde_json::to_string(&s)).transpose()?,
        ),
        // Nothing to count the language with, or no text: kept as such, so
        // that the book is not read again until its file, language or pack
        // changes.
        None => (0, 0, 0, 0, "{}".into(), "[]".into(), "[]".into(), None),
    };
    let at = crate::db::now_ts();
    let mut tx = state.db.begin().await?;
    // The candidates counted in vocabulary_df so far: those of the indexed
    // computation, when there is one.
    let old: Option<(String,)> = sqlx::query_as(
        "SELECT v.candidates FROM vocabulary v
         JOIN vocabulary_indexed i ON i.file_sha256 = v.file_sha256 AND i.language = v.language
         WHERE v.file_sha256 = $1 AND v.language = $2",
    )
    .bind(sha)
    .bind(language)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO vocabulary (file_sha256, language, version, words, stems, hapax, sample_need, coverage, curve, candidates, study, computed_at, pack)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (file_sha256, language) DO UPDATE SET version = excluded.version, words = excluded.words,
             stems = excluded.stems, hapax = excluded.hapax, sample_need = excluded.sample_need, coverage = excluded.coverage,
             curve = excluded.curve, candidates = excluded.candidates, study = excluded.study, computed_at = excluded.computed_at,
             pack = excluded.pack",
    )
    .bind(sha)
    .bind(language)
    .bind(VERSION)
    .bind(words)
    .bind(stems)
    .bind(hapax)
    .bind(need)
    .bind(coverage)
    .bind(curve)
    .bind(&candidates)
    .bind(&study)
    .bind(&at)
    .bind(pack)
    .execute(&mut *tx)
    .await?;
    index_words(&mut tx, sha, language, study.as_deref(), &at, old.as_ref().map(|(c,)| c.as_str()), &candidates).await?;
    tx.commit().await?;
    Ok(())
}

/// Rows of vocabulary_words written in one statement.
const INDEX_BATCH: usize = 150;
/// Stems of vocabulary_df changed in one statement.
const DF_BATCH: usize = 500;

/// The stems of a candidates JSON, each once.
fn candidate_stems(json: &str) -> HashSet<String> {
    let list: Vec<Candidate> = serde_json::from_str(json).unwrap_or_default();
    list.into_iter().map(|c| c.stem).collect()
}

/// Count one book less for these stems, and forget a stem no book has.
async fn df_remove(tx: &mut Transaction<'_, Any>, language: &str, stems: &[&str]) -> anyhow::Result<()> {
    for chunk in stems.chunks(DF_BATCH) {
        let marks: Vec<String> = (0..chunk.len()).map(|i| format!("${}", i + 2)).collect();
        let marks = marks.join(", ");
        for sql in [
            format!("UPDATE vocabulary_df SET docs = docs - 1 WHERE language = $1 AND stem IN ({marks})"),
            format!("DELETE FROM vocabulary_df WHERE language = $1 AND docs <= 0 AND stem IN ({marks})"),
        ] {
            let mut q = sqlx::query(&sql).bind(language);
            for stem in chunk {
                q = q.bind(*stem);
            }
            q.execute(&mut **tx).await?;
        }
    }
    Ok(())
}

/// Count one book more for these stems.
async fn df_add(tx: &mut Transaction<'_, Any>, language: &str, stems: &[&str]) -> anyhow::Result<()> {
    for chunk in stems.chunks(DF_BATCH) {
        let rows: Vec<String> = (0..chunk.len()).map(|i| format!("($1, ${}, 1)", i + 2)).collect();
        let sql = format!(
            "INSERT INTO vocabulary_df (language, stem, docs) VALUES {} ON CONFLICT (language, stem) DO UPDATE SET docs = vocabulary_df.docs + 1",
            rows.join(", ")
        );
        let mut q = sqlx::query(&sql).bind(language);
        for stem in chunk {
            q = q.bind(*stem);
        }
        q.execute(&mut **tx).await?;
    }
    Ok(())
}

/// Put a book's study list in vocabulary_words, one row per word, in place
/// of what was there; move its candidates from `old_candidates` (those
/// counted in vocabulary_df so far, if any) to `new_candidates`; and note
/// which computation the rows come from. All within the caller's
/// transaction.
async fn index_words(
    tx: &mut Transaction<'_, Any>,
    sha: &str,
    language: &str,
    study: Option<&str>,
    computed_at: &str,
    old_candidates: Option<&str>,
    new_candidates: &str,
) -> anyhow::Result<()> {
    let list: Vec<StudyWord> = match study.map(serde_json::from_str) {
        Some(Ok(list)) => list,
        Some(Err(e)) => {
            // Marked done all the same: the list is what the computation
            // gave, and the next computation replaces it.
            tracing::warn!("vocabulary: the study list of {sha} ({language}) cannot be read, no words indexed: {e}");
            Vec::new()
        }
        None => Vec::new(),
    };
    // One row per stem. A name and a word spelt alike are two entries of
    // the list; the word is the one indexed, so the name's uses are neither
    // learnable nor counted among the names here.
    let words: HashSet<&str> = list.iter().filter(|w| !w.name).map(|w| w.stem.as_str()).collect();
    let rows: Vec<&StudyWord> = list.iter().filter(|w| !w.name || !words.contains(w.stem.as_str())).collect();
    sqlx::query("DELETE FROM vocabulary_words WHERE file_sha256 = $1 AND language = $2")
        .bind(sha)
        .bind(language)
        .execute(&mut **tx)
        .await?;
    for chunk in rows.chunks(INDEX_BATCH) {
        let mut sql = String::from("INSERT INTO vocabulary_words (file_sha256, language, stem, word, forms, n, name) VALUES ");
        for i in 0..chunk.len() {
            let b = i * 7;
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("(${}, ${}, ${}, ${}, ${}, ${}, ${})", b + 1, b + 2, b + 3, b + 4, b + 5, b + 6, b + 7));
        }
        let mut q = sqlx::query(&sql);
        for w in chunk {
            q = q.bind(sha).bind(language).bind(&w.stem).bind(&w.word).bind(w.forms.join(" ")).bind(w.count as i64).bind(w.name as i64);
        }
        q.execute(&mut **tx).await?;
    }
    let old = old_candidates.map(candidate_stems).unwrap_or_default();
    let new = candidate_stems(new_candidates);
    let gone: Vec<&str> = old.difference(&new).map(String::as_str).collect();
    let came: Vec<&str> = new.difference(&old).map(String::as_str).collect();
    df_remove(tx, language, &gone).await?;
    df_add(tx, language, &came).await?;
    sqlx::query(
        "INSERT INTO vocabulary_indexed (file_sha256, language, computed_at) VALUES ($1, $2, $3)
         ON CONFLICT (file_sha256, language) DO UPDATE SET computed_at = excluded.computed_at",
    )
    .bind(sha)
    .bind(language)
    .bind(computed_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Index a few books whose study lists are not in vocabulary_words (counted
/// by a version without the table). Returns how many it did; a book that
/// cannot be indexed is reported and left for the next round.
pub(crate) async fn index_pending(state: &AppState, limit: i64) -> anyhow::Result<usize> {
    // The keys alone first: the row's text columns are read only for the
    // books to index.
    let keys: Vec<(String, String)> = sqlx::query_as(
        "SELECT v.file_sha256, v.language FROM vocabulary v
         WHERE NOT EXISTS (SELECT 1 FROM vocabulary_indexed i WHERE i.file_sha256 = v.file_sha256 AND i.language = v.language)
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let mut done = 0usize;
    for (sha, language) in &keys {
        match index_one(state, sha, language).await {
            Ok(()) => done += 1,
            Err(e) => tracing::warn!("vocabulary words: {sha} ({language}) could not be indexed: {e:#}"),
        }
    }
    Ok(done)
}

async fn index_one(state: &AppState, sha: &str, language: &str) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    let row: Option<(Option<String>, String, String)> =
        sqlx::query_as("SELECT study, computed_at, candidates FROM vocabulary WHERE file_sha256 = $1 AND language = $2")
            .bind(sha)
            .bind(language)
            .fetch_optional(&mut *tx)
            .await?;
    // Gone since the keys were read: nothing to index.
    let Some((study, at, candidates)) = row else { return Ok(()) };
    index_words(&mut tx, sha, language, study.as_deref(), &at, None, &candidates).await?;
    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------- backoff

/// Books whose file could not be read, by (file, language): how many times
/// and when last. Each is left alone for 2^attempts minutes (at most six
/// hours), so that a few books with a missing file do not fill every round
/// of their language. Kept per process: a restart tries again.
type Failures = HashMap<(String, String), (u32, Instant)>;

fn failures() -> &'static Mutex<Failures> {
    static F: OnceLock<Mutex<Failures>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

const HOLD_MAX: Duration = Duration::from_secs(6 * 3600);

fn hold(attempts: u32) -> Duration {
    Duration::from_secs(60u64 << attempts.min(16)).min(HOLD_MAX)
}

/// Whether a book's count is put off after failing.
fn held_back(sha: &str, language: &str) -> bool {
    failures().lock().unwrap().get(&(sha.to_string(), language.to_string())).is_some_and(|(attempts, at)| at.elapsed() < hold(*attempts))
}

/// Note a failed count; returns how many times it has failed.
fn count_failed(sha: &str, language: &str) -> u32 {
    let mut map = failures().lock().unwrap();
    let entry = map.entry((sha.to_string(), language.to_string())).or_insert((0, Instant::now()));
    entry.0 += 1;
    entry.1 = Instant::now();
    entry.0
}

fn count_succeeded(sha: &str, language: &str) {
    failures().lock().unwrap().remove(&(sha.to_string(), language.to_string()));
}

/// Count the words of a few books not yet counted, or counted by an older
/// version or with another pack than the language has now. Returns how many
/// were counted and kept; a book whose file cannot be read is reported and
/// left for a later round.
pub(crate) async fn count_pending(state: &AppState, limit: i64) -> anyhow::Result<usize> {
    let languages: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT b.language FROM books b
         WHERE b.format = 'epub' AND b.file_sha256 IS NOT NULL AND b.language IS NOT NULL ORDER BY b.language",
    )
    .fetch_all(&state.db)
    .await?;
    let mut done = 0usize;
    let mut left = limit;
    for (language,) in languages {
        if left <= 0 {
            break;
        }
        let expected = pack_id(pack_for(state, &language).as_ref());
        // A few more than wanted, since those held back after failing are
        // passed over.
        let held = failures().lock().unwrap().keys().filter(|(_, l)| *l == language).count() as i64;
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT MIN(b.uuid), b.file_sha256 FROM books b
             WHERE b.format = 'epub' AND b.file_sha256 IS NOT NULL AND b.language = $1
               AND NOT EXISTS (SELECT 1 FROM vocabulary v
                               WHERE v.file_sha256 = b.file_sha256 AND v.language = b.language AND v.version = $2 AND v.pack = $3)
             GROUP BY b.file_sha256
             LIMIT $4",
        )
        .bind(&language)
        .bind(VERSION)
        .bind(&expected)
        .bind(left + held)
        .fetch_all(&state.db)
        .await?;
        for (uuid, sha) in rows {
            if left <= 0 {
                break;
            }
            if held_back(&sha, &language) {
                continue;
            }
            left -= 1;
            match count_one(state, &uuid, &sha, &language).await {
                Ok(()) => {
                    done += 1;
                    count_succeeded(&sha, &language);
                }
                Err(e) => match count_failed(&sha, &language) {
                    1 => tracing::warn!("vocabulary: book {uuid} ({language}) could not be counted: {e:#}"),
                    n => tracing::debug!("vocabulary: book {uuid} ({language}) could not be counted ({n} times): {e:#}"),
                },
            }
        }
    }
    Ok(done)
}

/// Count one book's words and keep them. An error is a file that could not
/// be read: nothing is stored, so that the book is tried again.
pub(super) async fn count_one(state: &AppState, uuid: &str, sha: &str, language: &str) -> anyhow::Result<()> {
    let path = crate::books::book_path(state, uuid, Format::Epub);
    anyhow::ensure!(state.storage.fetch(&path).await, "the file of book {uuid} is not there");
    let lang = language.to_string();
    let pack = pack_for(state, language);
    let pack_id = pack_id(pack.as_ref());
    let computed = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Computed>> {
        let sections = booktext::read_sections(Format::Epub, &path)?;
        let text: String = sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
        Ok(compute(&text, &lang, pack))
    })
    .await??;
    let stored = store(state, sha, language, &pack_id, computed).await;
    give_memory_back();
    stored
}

/// Hand the memory freed after counting back to the system. glibc keeps
/// what a blocking thread's arena held, so a long count would otherwise
/// leave the process at its peak.
pub(crate) fn give_memory_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases free memory held by the allocator.
    unsafe {
        libc::malloc_trim(0);
    }
}

// --------------------------------------------------------------- clean-up

/// Keys (file, language) removed in one transaction.
const DROP_BATCH: usize = 50;

/// Drop what no book's file and language point to any more, and the
/// summaries of readers who are gone. The rows of a book go key by key, a
/// few keys per transaction: one statement over every word of the instance
/// would hold the database for as long as it ran. Returns how many keys
/// were dropped.
pub(crate) async fn drop_orphans(state: &AppState) -> anyhow::Result<u64> {
    let mut keys: Vec<(String, String)> = sqlx::query_as(
        "SELECT v.file_sha256, v.language FROM vocabulary v
         WHERE NOT EXISTS (SELECT 1 FROM books b WHERE b.file_sha256 = v.file_sha256 AND b.language = v.language)",
    )
    .fetch_all(&state.db)
    .await?;
    // An indexed row without its vocabulary row cannot come from this
    // version, but is cleaned up all the same.
    let stray: Vec<(String, String)> = sqlx::query_as(
        "SELECT i.file_sha256, i.language FROM vocabulary_indexed i
         WHERE NOT EXISTS (SELECT 1 FROM vocabulary v WHERE v.file_sha256 = i.file_sha256 AND v.language = i.language)",
    )
    .fetch_all(&state.db)
    .await?;
    keys.extend(stray);
    for chunk in keys.chunks(DROP_BATCH) {
        let mut tx = state.db.begin().await?;
        for (sha, language) in chunk {
            let counted: Option<(String,)> = sqlx::query_as(
                "SELECT v.candidates FROM vocabulary v
                 JOIN vocabulary_indexed i ON i.file_sha256 = v.file_sha256 AND i.language = v.language
                 WHERE v.file_sha256 = $1 AND v.language = $2",
            )
            .bind(sha)
            .bind(language)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((candidates,)) = counted {
                let stems = candidate_stems(&candidates);
                let stems: Vec<&str> = stems.iter().map(String::as_str).collect();
                df_remove(&mut tx, language, &stems).await?;
            }
            for table in ["vocabulary_words", "vocabulary_indexed", "vocabulary"] {
                sqlx::query(&format!("DELETE FROM {table} WHERE file_sha256 = $1 AND language = $2"))
                    .bind(sha)
                    .bind(language)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
    }
    // The summaries of readers who are gone, and the shares of books that are.
    let gone: Vec<(i64, String)> = sqlx::query_as(
        "SELECT s.user_id, s.language FROM word_summaries s WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.id = s.user_id)",
    )
    .fetch_all(&state.db)
    .await?;
    for (user, language) in gone {
        sqlx::query("DELETE FROM word_sums WHERE user_id = $1 AND language = $2").bind(user).bind(&language).execute(&state.db).await?;
        sqlx::query("DELETE FROM word_summaries WHERE user_id = $1 AND language = $2").bind(user).bind(&language).execute(&state.db).await?;
        sqlx::query("DELETE FROM book_known WHERE user_id = $1").bind(user).execute(&state.db).await?;
    }
    sqlx::query("DELETE FROM book_known WHERE NOT EXISTS (SELECT 1 FROM books b WHERE b.id = book_known.book_id)")
        .execute(&state.db)
        .await?;
    Ok(keys.len() as u64)
}

pub async fn worker(state: AppState) {
    let mut last_sweep: Option<Instant> = None;
    let mut analyzed = false;
    let mut upgrade = crate::langcatalogue::Upgrade::default();
    loop {
        if last_sweep.is_none_or(|at| at.elapsed() >= SWEEP) {
            // Before the books are counted: a book counted with the built-in
            // pack of old waits for the pack from the catalogue.
            crate::langcatalogue::upgrade_built_in(&state, &mut upgrade).await;
            if let Err(e) = crate::language::normalize_catalog(&state).await {
                tracing::warn!("languages: {e:#}");
            }
            if let Err(e) = drop_orphans(&state).await {
                tracing::warn!("vocabulary: {e:#}");
            }
            if let Err(e) = crate::analysis::drop_orphans(&state).await {
                tracing::warn!("analysis: {e:#}");
            }
            if state.backend == Backend::Sqlite {
                // Keeps the planner's statistics current as the tables grow.
                if let Err(e) = sqlx::query("PRAGMA optimize").execute(&state.db).await {
                    tracing::warn!("vocabulary: PRAGMA optimize: {e:#}");
                }
            }
            last_sweep = Some(Instant::now());
        }
        let busy = match count_pending(&state, 5).await {
            Ok(n) => n > 0,
            Err(e) => {
                tracing::warn!("vocabulary: {e:#}");
                false
            }
        };
        let indexing = match index_pending(&state, 20).await {
            Ok(n) => n > 0,
            Err(e) => {
                tracing::warn!("vocabulary words: {e:#}");
                false
            }
        };
        if !busy && !indexing {
            if !analyzed && state.backend == Backend::Sqlite {
                // Once the books are counted: statistics for the planner,
                // from a sample of each index so that the database is not
                // held for the whole of a large table.
                // Both on one connection: the limit is set per connection.
                if let Err(e) = (&state.db).execute("PRAGMA analysis_limit = 1000; ANALYZE;").await {
                    tracing::warn!("vocabulary: ANALYZE: {e:#}");
                }
                analyzed = true;
            }
            tokio::time::sleep(IDLE).await;
        }
    }
}
