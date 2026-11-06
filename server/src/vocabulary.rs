//! A book's vocabulary, after Zipf: a few words make up most of any text.
//! For each EPUB (once per file and language) a worker counts the words,
//! takes each to its stem (Snowball; Esperanto by its regular endings), and
//! keeps how many of the commonest stems cover half, 80, 90 and 95 % of the
//! text, plus the stems that may characterise the book. Comparisons with
//! the other books in the same language on the instance are made when asked,
//! since they change as books come and go. For Esperanto the book also gets
//! a study list: its words in order of frequency, with ESPDIC's English.

use crate::auth::AuthUser;
use crate::booktext;
use crate::formats::Format;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// Raised when what is computed changes, so that every book is done again.
const VERSION: i64 = 4;
/// The fixed-size sample the difficulty is measured on, so that a long book
/// does not look harder only for being long.
const SAMPLE: usize = 20_000;
/// Books in a language before a book's difficulty among them is shown.
pub const RATED_FROM: i64 = 10;
/// Stems kept as candidates for "characteristic": at least this often.
const CANDIDATE_MIN: u32 = 5;
const CANDIDATES: usize = 2000;
/// Words on an Esperanto study list.
const STUDY: usize = 1500;
const IDLE: Duration = Duration::from_secs(30);
const SWEEP: Duration = Duration::from_secs(600);

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

// ---------------------------------------------------------------- stemming

/// A language's stemmer: Snowball's, or Esperanto's own endings.
pub enum Stemmer {
    Snowball(rust_stemmers::Stemmer),
    Esperanto,
}

impl Stemmer {
    pub fn for_language(code: &str) -> Option<Stemmer> {
        use rust_stemmers::Algorithm::*;
        let algorithm = match code {
            "eo" => return Some(Stemmer::Esperanto),
            "ar" => Arabic,
            "da" => Danish,
            "nl" => Dutch,
            "en" => English,
            "fi" => Finnish,
            "fr" => French,
            "de" => German,
            "el" => Greek,
            "hu" => Hungarian,
            "it" => Italian,
            "nb" | "nn" => Norwegian,
            "pt" => Portuguese,
            "ro" => Romanian,
            "ru" => Russian,
            "es" => Spanish,
            "sv" => Swedish,
            "ta" => Tamil,
            "tr" => Turkish,
            _ => return None,
        };
        Some(Stemmer::Snowball(rust_stemmers::Stemmer::create(algorithm)))
    }

    /// A lower-case word's stem.
    pub fn stem(&self, word: &str) -> String {
        match self {
            Stemmer::Snowball(s) => s.stem(word).into_owned(),
            Stemmer::Esperanto => esperanto_stem(word),
        }
    }
}

const EO_PRONOUNS: [&str; 10] = ["mi", "vi", "li", "ŝi", "ĝi", "ni", "ili", "si", "oni", "ci"];
/// Words that take no grammatical ending and must not lose a letter that
/// looks like one ("pri", "tre", "tro").
const EO_FIXED: &[&str] = &[
    "la", "kaj", "de", "en", "al", "ne", "ke", "sed", "por", "kun", "el", "pri", "sur", "sub", "tra", "ĉe", "ĝis", "post", "antaŭ",
    "ĉar", "se", "ankaŭ", "nur", "jam", "tre", "tro", "pli", "plej", "ol", "aŭ", "ĉu", "eĉ", "ja", "do", "tamen", "nun", "hodiaŭ",
    "morgaŭ", "hieraŭ", "ankoraŭ", "baldaŭ", "apenaŭ", "preskaŭ", "ĉirkaŭ", "kontraŭ", "laŭ", "malgraŭ", "krom", "inter", "per",
    "pro", "sen", "super", "apud", "ekster", "je", "po", "ĉi", "for", "ju", "des", "mem", "nek", "plu", "tuj", "ambaŭ", "jen",
    "jes", "ho", "ve", "unu", "du", "tri", "kvar", "kvin", "ses", "sep", "ok", "naŭ", "dek", "cent", "mil", "adiaŭ", "almenaŭ",
    "kvazaŭ", "ĉiam", "neniam", "ĉu", "ajn", "eble", "dum", "kiam", "tiam", "iam",
];

fn is_correlative(word: &str) -> bool {
    for prefix in ["neni", "ĉi", "ki", "ti", "i"] {
        if let Some(rest) = word.strip_prefix(prefix) {
            if ["u", "o", "a", "e", "am", "al", "el", "om", "es"].contains(&rest) {
                return true;
            }
        }
    }
    false
}

/// An Esperanto word's root: without the plural and accusative endings and
/// the ending of its word class. Pronouns keep their possessives with them
/// ("mia" is "mi"), correlatives and endless words stay whole.
pub fn esperanto_stem(word: &str) -> String {
    // Words without endings first: the j of "kaj" and "plej" is no plural.
    if EO_FIXED.contains(&word) {
        return word.to_string();
    }
    let mut bare = word;
    if bare.chars().count() > 2 {
        bare = bare.strip_suffix('n').unwrap_or(bare);
    }
    if bare.chars().count() > 2 {
        bare = bare.strip_suffix('j').unwrap_or(bare);
    }
    if EO_FIXED.contains(&bare) || is_correlative(bare) {
        return bare.to_string();
    }
    if EO_PRONOUNS.contains(&bare) {
        return bare.to_string();
    }
    if let Some(pronoun) = bare.strip_suffix('a').filter(|p| EO_PRONOUNS.contains(p)) {
        return pronoun.to_string();
    }
    for ending in ["as", "is", "os", "us", "u", "i", "o", "a", "e"] {
        if let Some(root) = bare.strip_suffix(ending) {
            if root.chars().count() >= 2 {
                return root.to_string();
            }
        }
    }
    bare.to_string()
}

/// The dictionary form of an Esperanto stem from the forms seen: verbs as
/// -i, nouns -o, adjectives -a, adverbs -e; a word without ending as it is.
fn esperanto_headword(stem: &str, forms: &[(String, u32)]) -> String {
    let Some((top, _)) = forms.first() else { return stem.to_string() };
    // Correlatives, pronouns and words without endings are their own headword ("ĉiu", not "ĉiuj").
    if EO_FIXED.contains(&stem) || is_correlative(stem) || EO_PRONOUNS.contains(&stem) {
        return stem.to_string();
    }
    if top == stem {
        return top.clone();
    }
    let mut classes: HashMap<char, u32> = HashMap::new();
    for (form, n) in forms {
        let Some(rest) = form.strip_prefix(stem) else { continue };
        let rest = rest.trim_end_matches('n').trim_end_matches('j');
        let class = match rest {
            "as" | "is" | "os" | "us" | "u" | "i" => 'i',
            "o" => 'o',
            "a" => 'a',
            "e" => 'e',
            _ => continue,
        };
        *classes.entry(class).or_default() += n;
    }
    match classes.into_iter().max_by_key(|(c, n)| (*n, *c)) {
        Some((class, _)) => format!("{stem}{class}"),
        None => top.clone(),
    }
}

// ------------------------------------------------------------------ words

/// The words of a text, as written: letters, with an apostrophe inside a
/// word kept ("don't", "l'Vivo" is "l'Vivo").
fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        let joins = (c == '\'' || c == '’')
            && start.is_some()
            && chars.get(i + 1).is_some_and(|(_, next)| next.is_alphabetic());
        if c.is_alphabetic() || joins {
            start.get_or_insert(at);
        } else if let Some(s) = start.take() {
            out.push(&text[s..at]);
        }
    }
    if let Some(s) = start {
        out.push(&text[s..]);
    }
    out
}

/// The book's own text: Project Gutenberg's licence before and after it cut away.
fn without_gutenberg(text: &str) -> &str {
    let mut text = text;
    let upper = text.to_uppercase();
    if upper.len() == text.len() {
        for marker in ["*** START OF THE PROJECT GUTENBERG", "*** START OF THIS PROJECT GUTENBERG", "***START OF THE PROJECT GUTENBERG"] {
            if let Some(at) = upper.find(marker) {
                if let Some(end) = upper[at + 3..].find("***") {
                    text = &text[at + 3 + end + 3..];
                }
                break;
            }
        }
        let upper = text.to_uppercase();
        if upper.len() == text.len() {
            for marker in ["*** END OF THE PROJECT GUTENBERG", "*** END OF THIS PROJECT GUTENBERG", "***END OF THE PROJECT GUTENBERG"] {
                if let Some(at) = upper.find(marker) {
                    text = &text[..at];
                    break;
                }
            }
        }
    }
    text
}

// ------------------------------------------------------------- computing

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Candidate {
    pub stem: String,
    pub label: String,
    pub count: u32,
    pub name: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StudyWord {
    pub stem: String,
    pub word: String,
    pub forms: Vec<String>,
    pub count: u32,
    pub name: bool,
}

#[derive(Debug)]
pub struct Computed {
    pub words: u64,
    pub stems: u64,
    pub hapax: u64,
    pub sample_need: u64,
    pub coverage: HashMap<String, u64>,
    pub curve: Vec<(u64, f64)>,
    pub candidates: Vec<Candidate>,
    pub study: Option<Vec<StudyWord>>,
}

/// How many of the commonest stems it takes to cover 90 % of these.
fn need_for_90(stems: &[u32]) -> u64 {
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for s in stems {
        *counts.entry(*s).or_default() += 1;
    }
    let mut sorted: Vec<u32> = counts.into_values().collect();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let target = stems.len() as f64 * 0.9;
    let mut sum = 0u64;
    for (i, n) in sorted.iter().enumerate() {
        sum += *n as u64;
        if sum as f64 >= target {
            return i as u64 + 1;
        }
    }
    sorted.len() as u64
}

pub fn compute(text: &str, language: &str) -> Option<Computed> {
    let stemmer = Stemmer::for_language(language)?;
    let text = without_gutenberg(text);
    let tokens = words(text);
    if tokens.is_empty() {
        return None;
    }
    // Each distinct word and stem is kept once; the running text as stem
    // numbers. A book of 200 000 words then makes a few thousand
    // allocations, not a few per word.
    let mut word_ids: HashMap<String, u32> = HashMap::new();
    let mut word_stem: Vec<u32> = Vec::new();
    let mut word_count: Vec<u32> = Vec::new();
    let mut stem_ids: HashMap<String, u32> = HashMap::new();
    let mut stem_names: Vec<String> = Vec::new();
    let mut stem_inside: Vec<u32> = Vec::new();
    let mut stem_capitals: Vec<u32> = Vec::new();
    let mut stems: Vec<u32> = Vec::with_capacity(tokens.len());
    let mut lower = String::new();
    let base = text.as_ptr() as usize;
    for token in &tokens {
        let at = token.as_ptr() as usize - base;
        // Names: capitalised where a sentence does not begin (mid-sentence
        // occurrences only, since every word is capitalised first in a sentence).
        let mid_sentence = text[..at]
            .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '“' | '”' | '‘' | '’' | '«' | '»' | '(' | '—' | '–' | '-'))
            .chars()
            .next_back()
            .is_some_and(|c| !matches!(c, '.' | '!' | '?' | ':' | '…'));
        lower.clear();
        lower.extend(token.chars().flat_map(char::to_lowercase));
        let word = match word_ids.get(lower.as_str()) {
            Some(&w) => w,
            None => {
                let stem = stemmer.stem(&lower);
                let s = match stem_ids.get(&stem) {
                    Some(&s) => s,
                    None => {
                        let s = stem_names.len() as u32;
                        stem_ids.insert(stem.clone(), s);
                        stem_names.push(stem);
                        stem_inside.push(0);
                        stem_capitals.push(0);
                        s
                    }
                };
                let w = word_stem.len() as u32;
                word_ids.insert(lower.clone(), w);
                word_stem.push(s);
                word_count.push(0);
                w
            }
        };
        word_count[word as usize] += 1;
        let s = word_stem[word as usize];
        if mid_sentence {
            stem_inside[s as usize] += 1;
            if token.chars().next().is_some_and(char::is_uppercase) {
                stem_capitals[s as usize] += 1;
            }
        }
        stems.push(s);
    }
    drop(tokens);
    // Per distinct stem: its count and the forms it was written in.
    let mut counts: HashMap<String, u32> = HashMap::with_capacity(stem_names.len());
    let mut forms: HashMap<String, HashMap<String, u32>> = HashMap::with_capacity(stem_names.len());
    for (word, w) in word_ids {
        let s = word_stem[w as usize] as usize;
        let n = word_count[w as usize];
        *counts.entry(stem_names[s].clone()).or_default() += n;
        forms.entry(stem_names[s].clone()).or_default().insert(word, n);
    }
    let inside: HashMap<&str, u32> = stem_names.iter().map(String::as_str).zip(stem_inside.iter().copied()).collect();
    let capitals: HashMap<&str, u32> = stem_names.iter().map(String::as_str).zip(stem_capitals.iter().copied()).collect();
    let total = stems.len() as u64;
    let mut ranked: Vec<(&String, &u32)> = counts.iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let mut coverage = HashMap::new();
    let mut curve = Vec::new();
    let targets = [50u64, 80, 90, 95, 98];
    let mut next_target = 0;
    let step = (ranked.len() / 200).max(1);
    let mut sum = 0u64;
    for (i, (_, n)) in ranked.iter().enumerate() {
        sum += **n as u64;
        while next_target < targets.len() && sum * 100 >= targets[next_target] * total {
            coverage.insert(targets[next_target].to_string(), i as u64 + 1);
            next_target += 1;
        }
        if i == 0 || (i + 1) % step == 0 || i + 1 == ranked.len() {
            curve.push((i as u64 + 1, (sum as f64 / total as f64 * 10_000.0).round() / 10_000.0));
        }
    }

    // German writes every noun with a capital, so a capital says nothing there.
    let names_tell = language != "de";
    let is_name = |stem: &str| {
        let mid = inside.get(stem).copied().unwrap_or(0);
        names_tell && mid >= 2 && capitals.get(stem).copied().unwrap_or(0) as f64 > mid as f64 * 0.8
    };
    let sorted_forms = |stem: &str| -> Vec<(String, u32)> {
        let mut f: Vec<(String, u32)> = forms[stem].iter().map(|(w, n)| (w.clone(), *n)).collect();
        f.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        f
    };
    let candidates = ranked
        .iter()
        .filter(|(_, n)| **n >= CANDIDATE_MIN)
        .take(CANDIDATES)
        .map(|(stem, n)| {
            let name = is_name(stem);
            let f = sorted_forms(stem);
            let mut label = if language == "eo" && !name { esperanto_headword(stem, &f) } else { f[0].0.clone() };
            if name {
                let mut c = label.chars();
                label = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or(label);
            }
            Candidate { stem: (*stem).clone(), label, count: **n, name }
        })
        .collect();
    let study = (language == "eo").then(|| {
        ranked
            .iter()
            // A lone letter is an elision ("l'") or an initial, nothing to learn.
            .filter(|(stem, _)| stem.chars().count() > 1)
            .take(STUDY)
            .map(|(stem, n)| {
                let f = sorted_forms(stem);
                StudyWord {
                    stem: (*stem).clone(),
                    word: esperanto_headword(stem, &f),
                    forms: f.into_iter().take(4).map(|(w, _)| w).collect(),
                    count: **n,
                    name: is_name(stem),
                }
            })
            .collect()
    });
    let sample_need = if stems.len() < SAMPLE {
        0
    } else {
        // The middle of a long book, away from its front and back matter.
        let start = if stems.len() > SAMPLE * 2 { stems.len() / 2 - SAMPLE / 2 } else { 0 };
        need_for_90(&stems[start..start + SAMPLE])
    };
    Some(Computed {
        words: total,
        stems: counts.len() as u64,
        hapax: counts.values().filter(|n| **n == 1).count() as u64,
        sample_need,
        coverage,
        curve,
        candidates,
        study,
    })
}

// ----------------------------------------------------------------- worker

async fn store(state: &AppState, sha: &str, language: &str, c: Option<Computed>) -> anyhow::Result<()> {
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
        // No stemmer for the language, or no text: kept as such, so that the
        // book is not read again until its file or language changes.
        None => (0, 0, 0, 0, "{}".into(), "[]".into(), "[]".into(), None),
    };
    sqlx::query(
        "INSERT INTO vocabulary (file_sha256, language, version, words, stems, hapax, sample_need, coverage, curve, candidates, study, computed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (file_sha256, language) DO UPDATE SET version = excluded.version, words = excluded.words,
             stems = excluded.stems, hapax = excluded.hapax, sample_need = excluded.sample_need, coverage = excluded.coverage,
             curve = excluded.curve, candidates = excluded.candidates, study = excluded.study, computed_at = excluded.computed_at",
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
    .bind(candidates)
    .bind(study)
    .bind(crate::db::now_ts())
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Count the words of a few books not yet counted (or counted by an older
/// version). Returns how many were done.
pub(crate) async fn count_pending(state: &AppState, limit: i64) -> anyhow::Result<usize> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT MIN(b.uuid), b.file_sha256, b.language FROM books b
         WHERE b.format = 'epub' AND b.file_sha256 IS NOT NULL AND b.language IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM vocabulary v
                           WHERE v.file_sha256 = b.file_sha256 AND v.language = b.language AND v.version = $1)
         GROUP BY b.file_sha256, b.language
         LIMIT $2",
    )
    .bind(VERSION)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let done = rows.len();
    for (uuid, sha, language) in rows {
        count_one(state, &uuid, &sha, &language).await?;
    }
    Ok(done)
}

/// Count one book's words and keep them.
async fn count_one(state: &AppState, uuid: &str, sha: &str, language: &str) -> anyhow::Result<()> {
    let path = crate::books::book_path(state, uuid, Format::Epub);
    state.storage.fetch(&path).await;
    let lang = language.to_string();
    let computed = tokio::task::spawn_blocking(move || -> Option<Computed> {
        let sections = booktext::read_sections(Format::Epub, &path).ok()?;
        let text: String = sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
        compute(&text, &lang)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("vocabulary: book {uuid} could not be counted: {e}");
        None
    });
    let stored = store(state, sha, language, computed).await;
    give_memory_back();
    stored
}

/// Hand the memory freed after counting back to the system. glibc keeps
/// what a blocking thread's arena held, so a long count would otherwise
/// leave the process at its peak.
fn give_memory_back() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only releases free memory held by the allocator.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Drop what no book's file and language point to any more.
pub(crate) async fn drop_orphans(state: &AppState) -> anyhow::Result<u64> {
    let gone = sqlx::query(
        "DELETE FROM vocabulary WHERE NOT EXISTS
           (SELECT 1 FROM books b WHERE b.file_sha256 = vocabulary.file_sha256 AND b.language = vocabulary.language)",
    )
    .execute(&state.db)
    .await?;
    Ok(gone.rows_affected())
}

pub async fn worker(state: AppState) {
    let mut last_sweep: Option<std::time::Instant> = None;
    loop {
        if last_sweep.is_none_or(|at| at.elapsed() >= SWEEP) {
            if let Err(e) = crate::language::normalize_catalog(&state).await {
                tracing::warn!("languages: {e:#}");
            }
            if let Err(e) = drop_orphans(&state).await {
                tracing::warn!("vocabulary: {e:#}");
            }
            last_sweep = Some(std::time::Instant::now());
        }
        let busy = match count_pending(&state, 5).await {
            Ok(n) => n > 0,
            Err(e) => {
                tracing::warn!("vocabulary: {e:#}");
                false
            }
        };
        if !busy {
            tokio::time::sleep(IDLE).await;
        }
    }
}

// ----------------------------------------------------------------- asking

/// How many texts in a language each candidate stem appears in, and how
/// many texts there are; kept until a text in the language is added or
/// counted again.
type Spread = (String, Arc<HashMap<String, u32>>, i64);
fn spreads() -> &'static Mutex<HashMap<String, Spread>> {
    static S: OnceLock<Mutex<HashMap<String, Spread>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn spread(state: &AppState, language: &str) -> anyhow::Result<(Arc<HashMap<String, u32>>, i64)> {
    let (n, last): (i64, Option<String>) =
        sqlx::query_as("SELECT COUNT(*), MAX(computed_at) FROM vocabulary WHERE language = $1 AND words > 0")
            .bind(language)
            .fetch_one(&state.db)
            .await?;
    let key = format!("{n}|{}", last.unwrap_or_default());
    if let Some((k, map, n)) = spreads().lock().unwrap().get(language) {
        if *k == key {
            return Ok((map.clone(), *n));
        }
    }
    let rows: Vec<(String,)> = sqlx::query_as("SELECT candidates FROM vocabulary WHERE language = $1 AND words > 0")
        .bind(language)
        .fetch_all(&state.db)
        .await?;
    let mut df: HashMap<String, u32> = HashMap::new();
    for (json,) in rows {
        let list: Vec<Candidate> = serde_json::from_str(&json).unwrap_or_default();
        for c in list {
            *df.entry(c.stem).or_default() += 1;
        }
    }
    let map = Arc::new(df);
    spreads().lock().unwrap().insert(language.to_string(), (key, map.clone(), n));
    Ok((map, n))
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
    /// ESPDIC's English, and the headword it was found under when the word
    /// had to be taken apart.
    pub english: Option<String>,
    pub found_as: Option<String>,
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
    pub names: Vec<String>,
    /// Esperanto: the words in order of frequency, and how much of the text
    /// (names left out) the reader's known words cover.
    pub study: Option<Vec<StudyRow>>,
    pub known_share: Option<f64>,
    pub study_source: Option<&'static str>,
}

type Row = (String, i64, i64, i64, i64, String, String, String, Option<String>);

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
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no vocabulary" }))).into_response();
    };
    // A book not counted yet (just added, or counted by an older version) is
    // counted now rather than when the worker comes by.
    let counted: Option<(i64,)> =
        match sqlx::query_as("SELECT version FROM vocabulary WHERE file_sha256 = $1 AND language = $2")
            .bind(&sha)
            .bind(&language)
            .fetch_optional(&state.db)
            .await
        {
            Ok(c) => c,
            Err(e) => return internal(e),
        };
    if format == "epub" && counted.is_none_or(|(v,)| v != VERSION) {
        if let Err(e) = count_one(&state, &uuid, &sha, &language).await {
            return internal(e);
        }
    }
    let row: Option<Row> = match sqlx::query_as(
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
    let Some((language, words, stems, hapax, need, coverage, curve, candidates, study)) = row else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no vocabulary" }))).into_response();
    };
    match answer(&state, user.0.id, language, words, stems, hapax, need, &coverage, &curve, &candidates, study.as_deref()).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => internal(e),
    }
}

#[allow(clippy::too_many_arguments)]
async fn answer(
    state: &AppState,
    user: i64,
    language: String,
    words: i64,
    stems: i64,
    hapax: i64,
    need: i64,
    coverage: &str,
    curve: &str,
    candidates: &str,
    study: Option<&str>,
) -> anyhow::Result<Vocabulary> {
    let coverage: HashMap<String, i64> = serde_json::from_str(coverage)?;
    let curve: Vec<(i64, f64)> = serde_json::from_str(curve)?;
    let candidates: Vec<Candidate> = serde_json::from_str(candidates)?;
    let (df, books) = spread(state, &language).await?;

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

    // Characteristic: common here, rare in the language's other books. With
    // fewer than three books there is nothing to set it against.
    let mut characteristic = Vec::new();
    if books >= 3 {
        let mut scored: Vec<(f64, &Candidate)> = candidates
            .iter()
            .filter(|c| !c.name && c.label.chars().count() > 1 && !c.label.contains(['\'', '’']))
            .filter(|c| df.get(&c.stem).copied().unwrap_or(1) as i64 * 2 <= books)
            .map(|c| (c.count as f64 * ((books as f64 + 1.0) / (df.get(&c.stem).copied().unwrap_or(1) as f64 + 0.5)).ln(), c))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        characteristic = scored.into_iter().take(12).map(|(_, c)| c.label.clone()).collect();
    }
    // A name is the book's own: one found in most books of the language is a
    // word that is always capitalised ("I", "Mr") instead.
    let names: Vec<String> = candidates
        .iter()
        .filter(|c| c.name && c.label.chars().count() > 1)
        .filter(|c| books < 3 || df.get(&c.stem).copied().unwrap_or(1) as i64 * 2 <= books)
        .take(8)
        .map(|c| c.label.clone())
        .collect();

    let (study, known_share, study_source) = match study {
        Some(json) => {
            let list: Vec<StudyWord> = serde_json::from_str(json)?;
            let known: Vec<(String,)> = sqlx::query_as("SELECT stem FROM known_words WHERE user_id = $1 AND language = $2")
                .bind(user)
                .bind(&language)
                .fetch_all(&state.db)
                .await?;
            let known: std::collections::HashSet<String> = known.into_iter().map(|(s,)| s).collect();
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
                    let found = if w.name { None } else { crate::espdic::lookup(&w.word) };
                    StudyRow {
                        english: found.as_ref().map(|f| f.english.clone()),
                        found_as: found.filter(|f| f.derived).map(|f| f.headword),
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
            (Some(rows), Some(covered as f64 / learnable as f64), Some(crate::espdic::ATTRIBUTION))
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
        names,
        study,
        known_share,
        study_source,
    })
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
    if change.stem.is_empty() || change.stem.chars().count() > 80 || change.language.is_empty() || change.language.len() > 12 {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "a stem and a language" }))).into_response();
    }
    let result = if change.known {
        sqlx::query(
            "INSERT INTO known_words (user_id, language, stem) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, language, stem) DO NOTHING",
        )
        .bind(user.0.id)
        .bind(&change.language)
        .bind(&change.stem)
        .execute(&state.db)
        .await
    } else {
        sqlx::query("DELETE FROM known_words WHERE user_id = $1 AND language = $2 AND stem = $3")
            .bind(user.0.id)
            .bind(&change.language)
            .bind(&change.stem)
            .execute(&state.db)
            .await
    };
    match result {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esperanto_words_come_to_their_roots() {
        for (word, stem) in [
            ("diris", "dir"),
            ("diri", "dir"),
            ("domojn", "dom"),
            ("bona", "bon"),
            ("bone", "bon"),
            ("mia", "mi"),
            ("min", "mi"),
            ("ilin", "ili"),
            ("kiu", "kiu"),
            ("kiujn", "kiu"),
            ("pri", "pri"),
            ("tre", "tre"),
            ("tro", "tro"),
            ("la", "la"),
            ("kaj", "kaj"),
            ("plej", "plej"),
            ("ajn", "ajn"),
        ] {
            assert_eq!(esperanto_stem(word), stem, "{word}");
        }
    }

    #[test]
    fn words_keep_an_apostrophe_inside_but_not_around() {
        assert_eq!(words("«Don't», she said: 'kaj' l’Vivo."), vec!["Don't", "she", "said", "kaj", "l’Vivo"]);
    }

    #[test]
    fn coverage_follows_zipf_and_names_are_told_apart() {
        let mut text = String::new();
        for i in 0..400 {
            text.push_str("La hundo diris al Petro ke la kato bone dormas. ");
            if i % 4 == 0 {
                text.push_str("Petro respondis: la domo estas granda. ");
            }
        }
        let c = compute(&text, "eo").unwrap();
        assert!(c.words > 3000);
        assert!(c.coverage["50"] <= c.coverage["90"]);
        let petro = c.candidates.iter().find(|x| x.stem == "petr").unwrap();
        assert!(petro.name);
        assert_eq!(petro.label, "Petro");
        let study = c.study.unwrap();
        assert_eq!(study[0].word, "la");
        let diri = study.iter().find(|w| w.stem == "dir").unwrap();
        assert_eq!(diri.word, "diri");
        assert_eq!(c.sample_need, 0, "shorter than the sample: not compared");
    }

    #[test]
    fn a_language_without_a_stemmer_has_no_vocabulary() {
        assert!(compute("some words here", "zh").is_none());
        assert!(compute("några ord här", "sv").is_some());
    }

    #[test]
    fn gutenbergs_licence_is_not_counted() {
        let text = "Header *** START OF THE PROJECT GUTENBERG EBOOK X *** the story *** END OF THE PROJECT GUTENBERG EBOOK X *** licence";
        assert_eq!(without_gutenberg(text).trim(), "the story");
    }
}
