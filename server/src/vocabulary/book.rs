//! One book's vocabulary as the reader sees it: its numbers set against the
//! other books in the language (how many of them share each of its
//! candidate words comes from `vocabulary_df`), its study list with meanings
//! and the reader's known words, and the known words themselves.

use super::count::Stemmer;
use super::index::count_one;
use super::{bad_request, internal, not_found, pack_for, pack_id, Candidate, Glossary, Meaning, StudyWord, LANGUAGE_MAX, VERSION};
use crate::auth::AuthUser;
use crate::esperanto;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

/// Books in a language before a book's difficulty among them is shown.
pub const RATED_FROM: i64 = 10;
/// Books in a language before its characteristic words can be told.
const DISTINCTIVE_FROM: i64 = 5;

/// A language's stopwords (stopwords-iso), as words and as stems, so that
/// a stem or a label can be looked up. Built once per language.
pub(crate) fn stopwords(language: &str) -> Arc<HashSet<String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<HashSet<String>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(set) = cache.lock().unwrap().get(language) {
        return set.clone();
    }
    let iso = match language {
        "nb" | "nn" => "no",
        other => other,
    };
    let mut set = HashSet::new();
    if let Some(words) = stop_words::lookup(iso) {
        let stemmer = Stemmer::for_language(language);
        for w in words {
            let w = w.to_lowercase();
            if let Some(s) = &stemmer {
                set.insert(s.stem(&w));
            }
            set.insert(w);
        }
    }
    // stopwords-iso's Esperanto list is short: the words without endings,
    // the pronouns and the correlatives are little words too.
    if language == "eo" {
        set.extend(esperanto::little_words());
    }
    let set = Arc::new(set);
    cache.lock().unwrap().insert(language.to_string(), set.clone());
    set
}

/// Stems asked of vocabulary_df in one statement.
const DF_BATCH: usize = 500;

/// How many counted books in a language there are, and in how many of them
/// each of these stems is a candidate word (vocabulary_df, kept by the
/// worker): a stem in none is absent.
async fn spread(state: &AppState, language: &str, stems: &[&str]) -> anyhow::Result<(HashMap<String, i64>, i64)> {
    let (books,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vocabulary WHERE language = $1 AND words > 0")
        .bind(language)
        .fetch_one(&state.db)
        .await?;
    let mut df: HashMap<String, i64> = HashMap::with_capacity(stems.len());
    for chunk in stems.chunks(DF_BATCH) {
        let marks: Vec<String> = (0..chunk.len()).map(|i| format!("${}", i + 2)).collect();
        let sql = format!("SELECT stem, docs FROM vocabulary_df WHERE language = $1 AND stem IN ({})", marks.join(", "));
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql).bind(language);
        for stem in chunk {
            q = q.bind(*stem);
        }
        df.extend(q.fetch_all(&state.db).await?);
    }
    Ok((df, books))
}

#[derive(Serialize)]
pub struct Difficulty {
    /// 1 (much easier than most) to 5 (much harder).
    pub level: u32,
    /// The other books in the language it was compared with.
    pub compared_with: i64,
}

#[derive(Serialize)]
pub struct StudyRow {
    pub stem: String,
    pub word: String,
    pub forms: Vec<String>,
    pub count: u32,
    pub name: bool,
    pub known: bool,
    /// The glossary's English, and the headword it was found under when the
    /// word had to be taken apart (ESPDIC).
    pub english: Option<String>,
    pub found_as: Option<String>,
    /// "verb", "noun", "adj" or "other" (little words, adverbs and the rest);
    /// none for a name or a word without a known class.
    pub class: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Vocabulary {
    pub language: String,
    pub words: i64,
    pub stems: i64,
    pub hapax: i64,
    pub coverage: HashMap<String, i64>,
    pub curve: Vec<(i64, f64)>,
    /// Books in the same language on the instance, this one included.
    pub books_in_language: i64,
    pub rated_from: i64,
    /// None until there are enough books in the language, or when the book
    /// is too short to compare.
    pub difficulty: Option<Difficulty>,
    pub short: bool,
    pub characteristic: Vec<String>,
    /// The book's commonest words, without the language's stopwords.
    pub common: Vec<String>,
    /// Books in a language before characteristic words are given.
    pub distinctive_from: i64,
    pub names: Vec<String>,
    /// With a glossary: the words in order of frequency, and how much of the
    /// text (names left out) the reader's known words cover.
    pub study: Option<Vec<StudyRow>>,
    pub known_share: Option<f64>,
    pub study_source: Option<String>,
}

/// A book's row of `vocabulary`, as counted.
#[derive(sqlx::FromRow)]
struct Stored {
    language: String,
    words: i64,
    stems: i64,
    hapax: i64,
    sample_need: i64,
    coverage: String,
    curve: String,
    candidates: String,
    study: Option<String>,
}

pub async fn for_book(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Response {
    let book: Option<(String, String, Option<String>, Option<String>)> =
        match sqlx::query_as("SELECT uuid, format, file_sha256, language FROM books WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(user.0.id)
            .fetch_optional(&state.db)
            .await
        {
            Ok(b) => b,
            Err(e) => return internal(e),
        };
    let Some((uuid, format, Some(sha), Some(language))) = book else {
        return not_found("no vocabulary");
    };
    // A book not counted yet (just added, or counted by an older version or
    // with another pack) is counted now rather than when the worker comes by.
    let counted: Option<(i64, String)> =
        match sqlx::query_as("SELECT version, pack FROM vocabulary WHERE file_sha256 = $1 AND language = $2")
            .bind(&sha)
            .bind(&language)
            .fetch_optional(&state.db)
            .await
        {
            Ok(c) => c,
            Err(e) => return internal(e),
        };
    let expected = pack_id(pack_for(&state, &language).as_ref());
    if format == "epub" && counted.is_none_or(|(v, p)| v != VERSION || p != expected) {
        if let Err(e) = count_one(&state, &uuid, &sha, &language).await {
            return internal(e);
        }
    }
    let stored: Option<Stored> = match sqlx::query_as(
        "SELECT language, words, stems, hapax, sample_need, coverage, curve, candidates, study
         FROM vocabulary WHERE file_sha256 = $1 AND language = $2 AND words > 0",
    )
    .bind(&sha)
    .bind(&language)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let Some(stored) = stored else {
        return not_found("no vocabulary");
    };
    match answer(&state, user.0.id, stored).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => internal(e),
    }
}

async fn answer(state: &AppState, user: i64, stored: Stored) -> anyhow::Result<Vocabulary> {
    let Stored { language, words, stems, hapax, sample_need: need, coverage, curve, candidates, study } = stored;
    let coverage: HashMap<String, i64> = serde_json::from_str(&coverage)?;
    let curve: Vec<(i64, f64)> = serde_json::from_str(&curve)?;
    let candidates: Vec<Candidate> = serde_json::from_str(&candidates)?;
    let keys: Vec<&str> = candidates.iter().map(|c| c.stem.as_str()).collect();
    let (df, books) = spread(state, &language, &keys).await?;

    // Difficulty: where the book's sample need falls among the others'.
    let difficulty = if need > 0 && books >= RATED_FROM {
        let needs: Vec<(i64,)> =
            sqlx::query_as("SELECT sample_need FROM vocabulary WHERE language = $1 AND words > 0 AND sample_need > 0")
                .bind(&language)
                .fetch_all(&state.db)
                .await?;
        let at_most = needs.iter().filter(|(n,)| *n <= need).count() as f64;
        let share = at_most / needs.len().max(1) as f64;
        (needs.len() as i64 >= RATED_FROM).then(|| Difficulty {
            level: ((share * 5.0).ceil() as u32).clamp(1, 5),
            compared_with: needs.len() as i64 - 1,
        })
    } else {
        None
    };

    // The book's own words, without the language's stopwords. With enough
    // books in the language: the characteristic ones, common here and rare
    // in the others; with fewer, simply the commonest.
    let stop = stopwords(&language);
    let content = |c: &&Candidate| {
        c.label.chars().count() > 2 && !c.label.contains(['\'', '’']) && !stop.contains(&c.stem) && !stop.contains(&c.label.to_lowercase())
    };
    let mut common: Vec<&Candidate> = candidates.iter().filter(|c| !c.name).filter(content).collect();
    common.sort_by_key(|c| std::cmp::Reverse(c.count));
    let common: Vec<String> = common.into_iter().take(12).map(|c| c.label.clone()).collect();
    // Characteristic: common here, rare in the language's other books; none
    // until there are enough books to set them against. The common ones
    // are not repeated.
    let distinctive = books >= DISTINCTIVE_FROM;
    let mut characteristic = Vec::new();
    if distinctive {
        let mut scored: Vec<(f64, &Candidate)> = candidates
            .iter()
            .filter(|c| !c.name)
            .filter(content)
            .filter(|c| !common.contains(&c.label))
            .filter(|c| df.get(&c.stem).copied().unwrap_or(1) * 2 <= books)
            .map(|c| (c.count as f64 * ((books as f64 + 1.0) / (df.get(&c.stem).copied().unwrap_or(1) as f64 + 0.5)).ln(), c))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        characteristic = scored.into_iter().take(12).map(|(_, c)| c.label.clone()).collect();
    }
    // A name is the book's own: a stopword ("I") or one found in most books
    // of the language ("Mr") is a word that is always capitalised instead.
    let names: Vec<String> = candidates
        .iter()
        .filter(|c| c.name && c.label.chars().count() > 1 && !stop.contains(&c.stem) && !stop.contains(&c.label.to_lowercase()))
        .filter(|c| books < 3 || df.get(&c.stem).copied().unwrap_or(1) * 2 <= books)
        .take(8)
        .map(|c| c.label.clone())
        .collect();

    // Meanings from the language's pack; Esperanto has ESPDIC.
    let glossary = Glossary::for_language(state, &language);
    let (study, known_share, study_source) = match study {
        Some(json) => {
            let list: Vec<StudyWord> = serde_json::from_str(&json)?;
            let known: Vec<(String,)> = sqlx::query_as("SELECT stem FROM known_words WHERE user_id = $1 AND language = $2")
                .bind(user)
                .bind(&language)
                .fetch_all(&state.db)
                .await?;
            let known: HashSet<String> = known.into_iter().map(|(s,)| s).collect();
            // Share of the words there are to learn: names are left out of
            // both sides, so a reader who has ticked nothing starts at 0 %.
            let mut covered = 0u64;
            let mut names_count = 0u64;
            let rows: Vec<StudyRow> = list
                .into_iter()
                .map(|w| {
                    let is_known = known.contains(&w.stem);
                    if w.name {
                        names_count += w.count as u64;
                    } else if is_known {
                        covered += w.count as u64;
                    }
                    // A name is looked up too ("Dio", "Parizo" have their
                    // entries), but gets no class of its own without a hit.
                    let m = match glossary.meaning(&w.word, &w.stem) {
                        Meaning { english: None, .. } if w.name => Meaning::default(),
                        m => m,
                    };
                    StudyRow {
                        english: m.english,
                        found_as: m.found_as,
                        class: m.class,
                        stem: w.stem,
                        word: w.word,
                        forms: w.forms,
                        count: w.count,
                        name: w.name,
                        known: is_known,
                    }
                })
                .collect();
            let learnable = (words as u64).saturating_sub(names_count).max(1);
            (Some(rows), Some(covered as f64 / learnable as f64), glossary.attribution())
        }
        None => (None, None, None),
    };
    Ok(Vocabulary {
        language,
        words,
        stems,
        hapax,
        coverage,
        curve,
        books_in_language: books,
        rated_from: RATED_FROM,
        difficulty,
        short: need == 0,
        characteristic,
        common,
        distinctive_from: DISTINCTIVE_FROM,
        names,
        study,
        known_share,
        study_source,
    })
}

/// Whether a user has turned vocabulary and study lists on.
pub async fn turned_on(state: &AppState, user: i64) -> bool {
    sqlx::query_scalar::<_, crate::db::DbFlag>("SELECT words FROM users WHERE id = $1")
        .bind(user)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .is_some_and(|f| f.0 != 0)
}

// ------------------------------------------------------------ known words

#[derive(Deserialize)]
pub struct KnownQuery {
    language: String,
}

pub async fn known_list(State(state): State<AppState>, user: AuthUser, Query(q): Query<KnownQuery>) -> Response {
    let rows: Result<Vec<(String,)>, _> = sqlx::query_as("SELECT stem FROM known_words WHERE user_id = $1 AND language = $2")
        .bind(user.0.id)
        .bind(&q.language)
        .fetch_all(&state.db)
        .await;
    match rows {
        Ok(rows) => Json(json!({ "stems": rows.into_iter().map(|(s,)| s).collect::<Vec<_>>() })).into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Deserialize)]
pub struct KnownChange {
    language: String,
    stem: String,
    known: bool,
}

pub async fn known_set(State(state): State<AppState>, user: AuthUser, Json(change): Json<KnownChange>) -> Response {
    if change.stem.is_empty() || change.stem.chars().count() > 80 || change.language.is_empty() || change.language.len() > LANGUAGE_MAX {
        return bad_request("a stem and a language");
    }
    match set_one(&state, user.0.id, &change.language, &change.stem, change.known).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

/// Tick or untick one word; when that changes anything, the shares of the
/// reader's books on the words page move with it.
async fn set_one(state: &AppState, user: i64, language: &str, stem: &str, known: bool) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    let changed = if known {
        sqlx::query(
            "INSERT INTO known_words (user_id, language, stem) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, language, stem) DO NOTHING",
        )
        .bind(user)
        .bind(language)
        .bind(stem)
        .execute(&mut *tx)
        .await?
    } else {
        sqlx::query("DELETE FROM known_words WHERE user_id = $1 AND language = $2 AND stem = $3")
            .bind(user)
            .bind(language)
            .bind(stem)
            .execute(&mut *tx)
            .await?
    };
    if changed.rows_affected() == 1 {
        super::words::known_changed(&mut tx, user, language, stem, known).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Words one bulk change may tick or untick at most.
const MANY_MAX: usize = 5000;

#[derive(Deserialize)]
pub struct KnownMany {
    language: String,
    stems: Vec<String>,
    known: bool,
}

/// PUT /api/vocabulary/known/many: tick or untick many words at once (all
/// the words above one in a study list).
pub async fn known_many(State(state): State<AppState>, user: AuthUser, Json(change): Json<KnownMany>) -> Response {
    if change.language.is_empty() || change.language.len() > LANGUAGE_MAX || change.stems.len() > MANY_MAX {
        return bad_request("a language and at most 5000 stems");
    }
    let stems: Vec<&String> = change.stems.iter().filter(|s| !s.is_empty() && s.chars().count() <= 80).collect();
    match set_many(&state, user.0.id, &change.language, &stems, change.known).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

/// Stems one statement ticks or unticks at most (three parameters each
/// when ticking).
const SET_BATCH: usize = 600;

/// Tick or untick many stems in one transaction, a few hundred per
/// statement. The shares of the reader's books on the words page are
/// marked to be built again.
pub(super) async fn set_many(state: &AppState, user: i64, language: &str, stems: &[&String], known: bool) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    super::words::known_changed_many(&mut tx, user, language).await?;
    for chunk in stems.chunks(SET_BATCH) {
        let sql = if known {
            let rows: Vec<String> = (0..chunk.len()).map(|i| format!("($1, $2, ${})", i + 3)).collect();
            format!("INSERT INTO known_words (user_id, language, stem) VALUES {} ON CONFLICT (user_id, language, stem) DO NOTHING", rows.join(", "))
        } else {
            let marks: Vec<String> = (0..chunk.len()).map(|i| format!("${}", i + 3)).collect();
            format!("DELETE FROM known_words WHERE user_id = $1 AND language = $2 AND stem IN ({})", marks.join(", "))
        };
        let mut q = sqlx::query(&sql).bind(user).bind(language);
        for stem in chunk {
            q = q.bind(stem.as_str());
        }
        q.execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Whether the reader has ticked a stem.
async fn is_known(state: &AppState, user: i64, language: &str, stem: &str) -> anyhow::Result<bool> {
    let found: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM known_words WHERE user_id = $1 AND language = $2 AND stem = $3")
        .bind(user)
        .bind(language)
        .bind(stem)
        .fetch_optional(&state.db)
        .await?;
    Ok(found.is_some())
}

// ------------------------------------------------------------ in the reader

#[derive(Deserialize)]
pub struct WordQuery {
    w: String,
}

#[derive(Serialize)]
pub struct WordInBook {
    /// The key known words are kept by.
    pub stem: String,
    /// The dictionary form ("hacer" for "hizo").
    pub word: String,
    pub english: Option<String>,
    pub class: Option<&'static str>,
    pub known: bool,
    /// How often the book uses it, in all its forms.
    pub count: u32,
    /// Whether the language's words have tables of forms (analysis/).
    pub forms: bool,
}

/// GET /api/books/{id}/vocabulary/word?w=: a word selected in the reader,
/// as the book's study list has it: its dictionary form, meaning, class and
/// whether the reader knows it. 404 when the book has no study list or the
/// word is a name.
pub async fn word_in_book(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>, Query(q): Query<WordQuery>) -> Response {
    let not_on_list = || not_found("not on the study list");
    let row: Option<(String, String)> = match sqlx::query_as(
        "SELECT b.language, v.study FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         WHERE b.id = $1 AND b.owner_id = $2 AND v.study IS NOT NULL",
    )
    .bind(id)
    .bind(user.0.id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let Some((language, json)) = row else { return not_on_list() };
    let list: Vec<StudyWord> = serde_json::from_str(&json).unwrap_or_default();
    let written: String = q.w.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let written = if language == "eo" { esperanto::supersigned(&written).unwrap_or(written) } else { written };
    if written.is_empty() || written.chars().count() > 60 {
        return not_on_list();
    }
    // As the list has it: by one of its forms, else by the word's stem
    // (Esperanto) or lemma (a pack) when the form is not among those kept.
    let glossary = Glossary::for_language(&state, &language);
    let key = match &glossary {
        Glossary::Pack(p) => {
            let lemmas = p.lemmas_of(&written);
            lemmas
                .iter()
                .filter_map(|l| p.lemma(*l).map(|x| x.word.to_string()))
                .find(|w| list.iter().any(|s| &s.stem == w))
                .or_else(|| lemmas.first().and_then(|l| p.lemma(*l)).map(|x| x.word.to_string()))
        }
        Glossary::Espdic => Some(esperanto::stem(&written)),
        Glossary::None => None,
    };
    let found = list
        .iter()
        .find(|s| s.forms.contains(&written))
        .or_else(|| key.as_ref().and_then(|k| list.iter().find(|s| &s.stem == k)));
    let (stem, word, count, name) = match (found, &key) {
        (Some(s), _) => (s.stem.clone(), s.word.clone(), s.count, s.name),
        (None, Some(k)) => {
            let shown = match &glossary {
                Glossary::Pack(p) => p.find(k).map(|l| l.written.to_string()).unwrap_or_else(|| k.clone()),
                _ => crate::espdic::lookup(&written).map(|f| f.headword).unwrap_or_else(|| written.clone()),
            };
            (k.clone(), shown, 0, false)
        }
        (None, None) => return not_on_list(),
    };
    if name {
        return not_on_list();
    }
    let m = glossary.meaning(&word, &stem);
    let known = match is_known(&state, user.0.id, &language, &stem).await {
        Ok(k) => k,
        Err(e) => return internal(e),
    };
    let forms = crate::analysis::has_forms(&state, &language);
    Json(WordInBook { stem, word, english: m.english, class: m.class, known, count, forms }).into_response()
}
