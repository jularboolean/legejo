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
const VERSION: i64 = 6;
/// The fixed-size sample the difficulty is measured on, so that a long book
/// does not look harder only for being long.
const SAMPLE: usize = 20_000;
/// Books in a language before a book's difficulty among them is shown.
pub const RATED_FROM: i64 = 10;
/// Books in a language before its characteristic words can be told.
const DISTINCTIVE_FROM: i64 = 5;
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

pub fn compute(text: &str, language: &str, pack: Option<&crate::langpack::Pack>) -> Option<Computed> {
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
    // The distinct words with their counts and stems, kept for the study list.
    let mut written: Vec<(String, u32, u32)> = Vec::with_capacity(word_ids.len());
    for (word, w) in word_ids {
        let s = word_stem[w as usize] as usize;
        let n = word_count[w as usize];
        *counts.entry(stem_names[s].clone()).or_default() += n;
        forms.entry(stem_names[s].clone()).or_default().insert(word.clone(), n);
        written.push((word, n, s as u32));
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
    let study = if let Some(pack) = pack.filter(|_| language != "eo") {
        Some(study_by_lemma(pack, &written, &stem_names, &is_name))
    } else {
        (language == "eo").then(|| {
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
        })
    };
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

/// A study list from a language pack: each written word taken to its lemma
/// (a form of several lemmas to the one the book uses most in forms that can
/// only be that one, a little word to itself), lemmas in order of frequency.
/// A word the pack does not know stands as written; a name as a name.
fn study_by_lemma(
    pack: &crate::langpack::Pack,
    written: &[(String, u32, u32)],
    stem_names: &[String],
    is_name: &dyn Fn(&str) -> bool,
) -> Vec<StudyWord> {
    let found: Vec<Vec<u32>> = written.iter().map(|(w, _, _)| pack.lemmas_of(w)).collect();
    let mut sure: HashMap<u32, u32> = HashMap::new();
    for ((_, n, _), lemmas) in written.iter().zip(&found) {
        if let [one] = lemmas.as_slice() {
            *sure.entry(*one).or_default() += n;
        }
    }
    // Key: the lemma's word, or the written word when the pack has none.
    let mut by: HashMap<String, (u32, HashMap<String, u32>, bool)> = HashMap::new();
    let mut shown: HashMap<String, String> = HashMap::new();
    for ((word, n, stem), lemmas) in written.iter().zip(&found) {
        let name = is_name(&stem_names[*stem as usize]);
        let key = if name || lemmas.is_empty() {
            word.clone()
        } else {
            // A word that is a lemma itself stays itself ("a", "no", "casa")
            // unless the book uses another of its lemmas more in forms that
            // can only be that one ("era" as "ser", "hace" as "hacer").
            let own = lemmas.iter().copied().find(|l| {
                pack.lemma(*l).is_some_and(|x| x.word == word)
                    && lemmas.iter().filter(|o| *o != l).all(|o| sure.get(o).copied().unwrap_or(0) < *n)
            });
            let pick = own.unwrap_or_else(|| {
                *lemmas
                    .iter()
                    .max_by_key(|l| {
                        let w = pack.lemma(**l).map(|x| x.word).unwrap_or("");
                        // Then a content word over a little one, the longer over
                        // the shorter ("es" is "ser", not the conjunction "e").
                        (sure.get(l).copied().unwrap_or(0), w == word, !pack.is_little(**l), w.len())
                    })
                    .unwrap()
            });
            match pack.lemma(pick) {
                Some(x) => {
                    if x.written != x.word {
                        shown.insert(x.word.to_string(), x.written.to_string());
                    }
                    x.word.to_string()
                }
                None => word.clone(),
            }
        };
        let entry = by.entry(key).or_insert_with(|| (0, HashMap::new(), name));
        entry.0 += n;
        *entry.1.entry(word.clone()).or_default() += n;
    }
    // A lone letter the pack does not know is an elision or an initial.
    let mut list: Vec<(String, (u32, HashMap<String, u32>, bool))> =
        by.into_iter().filter(|(k, _)| k.chars().count() > 1 || pack.find(k).is_some()).collect();
    list.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    list.into_iter()
        .take(STUDY)
        .map(|(key, (count, forms, name))| {
            let mut f: Vec<(String, u32)> = forms.into_iter().collect();
            f.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let word = if name {
                let mut c = key.chars();
                c.next().map(|x| x.to_uppercase().chain(c).collect()).unwrap_or(key.clone())
            } else {
                shown.get(&key).cloned().unwrap_or_else(|| key.clone())
            };
            StudyWord { stem: key, word, forms: f.into_iter().take(4).map(|(w, _)| w).collect(), count, name }
        })
        .collect()
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
    let at = crate::db::now_ts();
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
    .bind(&study)
    .bind(&at)
    .execute(&state.db)
    .await?;
    index_words(state, sha, language, study.as_deref(), &at).await
}

/// Rows of vocabulary_words written in one statement.
const INDEX_BATCH: usize = 150;

/// Put a book's study list in vocabulary_words, one row per word, in place
/// of what was there, and note which computation it comes from.
async fn index_words(state: &AppState, sha: &str, language: &str, study: Option<&str>, computed_at: &str) -> anyhow::Result<()> {
    let list: Vec<StudyWord> = study.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM vocabulary_words WHERE file_sha256 = $1 AND language = $2")
        .bind(sha)
        .bind(language)
        .execute(&mut *tx)
        .await?;
    for chunk in list.chunks(INDEX_BATCH) {
        let mut sql = String::from("INSERT INTO vocabulary_words (file_sha256, language, stem, word, forms, n, name) VALUES ");
        for i in 0..chunk.len() {
            let b = i * 7;
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("(${}, ${}, ${}, ${}, ${}, ${}, ${})", b + 1, b + 2, b + 3, b + 4, b + 5, b + 6, b + 7));
        }
        sql.push_str(" ON CONFLICT (file_sha256, language, stem) DO NOTHING");
        let mut q = sqlx::query(&sql);
        for w in chunk {
            q = q.bind(sha).bind(language).bind(&w.stem).bind(&w.word).bind(w.forms.join(" ")).bind(w.count as i64).bind(w.name as i64);
        }
        q.execute(&mut *tx).await?;
    }
    sqlx::query(
        "INSERT INTO vocabulary_indexed (file_sha256, language, computed_at) VALUES ($1, $2, $3)
         ON CONFLICT (file_sha256, language) DO UPDATE SET computed_at = excluded.computed_at",
    )
    .bind(sha)
    .bind(language)
    .bind(computed_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Index a few books whose study lists are not in vocabulary_words yet (or
/// are there from an older computation, or were counted by a version
/// without the table). Returns how many it did.
pub(crate) async fn index_pending(state: &AppState, limit: i64) -> anyhow::Result<usize> {
    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT v.file_sha256, v.language, v.study, v.computed_at FROM vocabulary v
         LEFT JOIN vocabulary_indexed i ON i.file_sha256 = v.file_sha256 AND i.language = v.language
         WHERE i.computed_at IS NULL OR i.computed_at <> v.computed_at
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    for (sha, language, study, at) in &rows {
        index_words(state, sha, language, study.as_deref(), at).await?;
    }
    Ok(rows.len())
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
    let pack = crate::langpack::get(&state.data_dir, language);
    let computed = tokio::task::spawn_blocking(move || -> Option<Computed> {
        let sections = booktext::read_sections(Format::Epub, &path).ok()?;
        let text: String = sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
        compute(&text, &lang, pack.as_deref())
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
    for table in ["vocabulary_words", "vocabulary_indexed"] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE NOT EXISTS
               (SELECT 1 FROM vocabulary v WHERE v.file_sha256 = {table}.file_sha256 AND v.language = {table}.language)"
        ))
        .execute(&state.db)
        .await?;
    }
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
        let indexing = match index_pending(&state, 20).await {
            Ok(n) => n > 0,
            Err(e) => {
                tracing::warn!("vocabulary words: {e:#}");
                false
            }
        };
        let busy = busy || indexing;
        if !busy {
            tokio::time::sleep(IDLE).await;
        }
    }
}

// ----------------------------------------------------------------- asking

/// A language's stopwords (stopwords-iso), as words and as stems, so that
/// a stem or a label can be looked up.
fn stopwords(language: &str) -> std::collections::HashSet<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<std::collections::HashSet<String>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(set) = cache.lock().unwrap().get(language) {
        return (**set).clone();
    }
    let iso = match language {
        "nb" | "nn" => "no",
        other => other,
    };
    let mut set = std::collections::HashSet::new();
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
        set.extend(EO_FIXED.iter().chain(EO_PRONOUNS.iter()).map(|w| w.to_string()));
        for prefix in ["neni", "ĉi", "ki", "ti", "i"] {
            for end in ["u", "o", "a", "e", "am", "al", "el", "om", "es"] {
                set.insert(format!("{prefix}{end}"));
            }
        }
    }
    cache.lock().unwrap().insert(language.to_string(), Arc::new(set.clone()));
    set
}

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
    /// "verb", "noun", "adj" or "other" (little words, adverbs and the rest);
    /// none for a name or a word without a known class.
    pub class: Option<&'static str>,
}

/// A word class to filter a study list by, from a part of speech.
fn class_of_pos(pos: &str) -> &'static str {
    match pos {
        "verb" => "verb",
        "noun" => "noun",
        "adj" => "adj",
        _ => "other",
    }
}

/// An Esperanto headword's class, by its ending.
fn esperanto_class(word: &str) -> &'static str {
    if EO_FIXED.contains(&word) || EO_PRONOUNS.contains(&word) || is_correlative(word) || word.chars().count() < 3 {
        return "other";
    }
    match word.chars().last() {
        Some('i') => "verb",
        Some('o') => "noun",
        Some('a') => "adj",
        _ => "other",
    }
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
    /// Esperanto: the words in order of frequency, and how much of the text
    /// (names left out) the reader's known words cover.
    pub study: Option<Vec<StudyRow>>,
    pub known_share: Option<f64>,
    pub study_source: Option<String>,
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

    // The book's own words, without the language's stopwords. With enough
    // books in the language: the characteristic ones, common here and rare
    // in the others; with fewer, simply the commonest.
    let stop = stopwords(&language);
    let content = |c: &&Candidate| {
        c.label.chars().count() > 2 && !c.label.contains(['\'', '’']) && !stop.contains(&c.stem) && !stop.contains(&c.label.to_lowercase())
    };
    let mut common: Vec<&Candidate> = candidates.iter().filter(|c| !c.name).filter(content).collect();
    common.sort_by(|a, b| b.count.cmp(&a.count));
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
            .filter(|c| df.get(&c.stem).copied().unwrap_or(1) as i64 * 2 <= books)
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
        .filter(|c| books < 3 || df.get(&c.stem).copied().unwrap_or(1) as i64 * 2 <= books)
        .take(8)
        .map(|c| c.label.clone())
        .collect();

    // Meanings from the language's pack; Esperanto has ESPDIC.
    let pack = if language == "eo" { None } else { crate::langpack::get(&state.data_dir, &language) };
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
                    let (english, found_as, class) = if w.name {
                        (None, None, None)
                    } else if let Some(pack) = &pack {
                        let lemma = pack.find(&w.stem);
                        (
                            lemma.as_ref().map(|l| l.english.to_string()).filter(|e| !e.is_empty()),
                            None,
                            lemma.and_then(|l| l.pos.split(',').next().map(class_of_pos)),
                        )
                    } else {
                        let found = crate::espdic::lookup(&w.word);
                        (
                            found.as_ref().map(|f| f.english.clone()),
                            found.filter(|f| f.derived).map(|f| f.headword),
                            Some(esperanto_class(&w.word)),
                        )
                    };
                    StudyRow {
                        english,
                        found_as,
                        class,
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
            let source = match &pack {
                Some(p) => p.meta.attribution.clone(),
                None => crate::espdic::ATTRIBUTION.to_string(),
            };
            (Some(rows), Some(covered as f64 / learnable as f64), Some(source))
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
    if change.language.is_empty() || change.language.len() > 12 || change.stems.len() > MANY_MAX {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "a language and at most 5000 stems" }))).into_response();
    }
    let stems: Vec<&String> = change.stems.iter().filter(|s| !s.is_empty() && s.chars().count() <= 80).collect();
    match set_many(&state, user.0.id, &change.language, &stems, change.known).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

async fn set_many(state: &AppState, user: i64, language: &str, stems: &[&String], known: bool) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    for stem in stems {
        let query = if known {
            "INSERT INTO known_words (user_id, language, stem) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, language, stem) DO NOTHING"
        } else {
            "DELETE FROM known_words WHERE user_id = $1 AND language = $2 AND stem = $3"
        };
        sqlx::query(query).bind(user).bind(language).bind(stem.as_str()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

// ------------------------------------------------------------ level test

/// The frequency bands of the level test, by rank among the words of the
/// reader's books in the language, and the words asked from each.
const LEVEL_BANDS: [usize; 7] = [0, 250, 500, 1000, 2000, 4000, 8000];
const LEVEL_ASK: usize = 8;

/// The words of the reader's books in a language, names aside, commonest
/// first, as far as the level test reaches.
async fn ranked_words(state: &AppState, user: i64, language: &str) -> anyhow::Result<Vec<WordRow>> {
    let rows: Vec<Total> = sqlx::query_as(&format!(
        "SELECT w.stem, MIN(w.word), CAST(SUM(w.n) AS BIGINT) AS total, COUNT(*) {READERS_WORDS}
         GROUP BY w.stem ORDER BY total DESC, w.stem LIMIT {}",
        LEVEL_BANDS[LEVEL_BANDS.len() - 1]
    ))
    .bind(user)
    .bind(language)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|t| row_of(t, false)).collect())
}

/// The bands that the reader's words reach: (from, to) by rank.
fn level_bands(words: usize) -> Vec<(usize, usize)> {
    LEVEL_BANDS.windows(2).filter(|b| b[0] < words).map(|b| (b[0], b[1].min(words))).collect()
}

#[derive(Deserialize)]
pub struct LevelQuery {
    language: String,
}

#[derive(Serialize)]
pub struct LevelWord {
    pub stem: String,
    pub word: String,
    /// Shown once the reader has said they know the word, to check it.
    pub english: Option<String>,
}

#[derive(Serialize)]
pub struct LevelBand {
    pub from: usize,
    pub to: usize,
    pub words: Vec<LevelWord>,
}

/// GET /api/vocabulary/level?language=: words to ask in a short level test,
/// a few picked at random from each frequency band of the reader's books.
pub async fn level_sample(State(state): State<AppState>, user: AuthUser, Query(q): Query<LevelQuery>) -> Response {
    use rand::seq::SliceRandom;
    let ranked = match ranked_words(&state, user.0.id, &q.language).await {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let mut rng = rand::thread_rng();
    let bands: Vec<LevelBand> = level_bands(ranked.len())
        .into_iter()
        .map(|(from, to)| {
            let mut words: Vec<LevelWord> = ranked[from..to]
                .choose_multiple(&mut rng, LEVEL_ASK)
                .map(|w| LevelWord { stem: w.stem.clone(), word: w.word.clone(), english: meaning_of(&state, &q.language, &w.word, &w.stem).0 })
                .collect();
            words.shuffle(&mut rng);
            LevelBand { from, to, words }
        })
        .collect();
    Json(json!({ "words": ranked.len(), "bands": bands })).into_response()
}

#[derive(Deserialize)]
pub struct LevelAnswer {
    stem: String,
    known: bool,
}

#[derive(Deserialize)]
pub struct LevelAnswers {
    language: String,
    answers: Vec<LevelAnswer>,
}

/// POST /api/vocabulary/level: the answers of a level test. In each band
/// the reader is taken to know as large a share of the words as they knew
/// of those asked, the commonest first; the words answered are ticked or
/// left as answered. Words ticked before stay ticked.
pub async fn level_apply(State(state): State<AppState>, user: AuthUser, Json(a): Json<LevelAnswers>) -> Response {
    if a.answers.len() > 200 {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "at most 200 answers" }))).into_response();
    }
    let ranked = match ranked_words(&state, user.0.id, &a.language).await {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let rank: HashMap<&str, usize> = ranked.iter().enumerate().map(|(i, w)| (w.stem.as_str(), i)).collect();
    let answer: HashMap<&str, bool> = a.answers.iter().map(|x| (x.stem.as_str(), x.known)).collect();
    let mut tick: Vec<&String> = Vec::new();
    let mut estimate = 0usize;
    for (from, to) in level_bands(ranked.len()) {
        let asked: Vec<bool> = answer.iter().filter(|(s, _)| rank.get(*s).is_some_and(|r| (from..to).contains(r))).map(|(_, k)| *k).collect();
        if asked.is_empty() {
            continue;
        }
        let share = asked.iter().filter(|k| **k).count() as f64 / asked.len() as f64;
        let take = ((to - from) as f64 * share).round() as usize;
        estimate += take;
        tick.extend(ranked[from..from + take].iter().map(|w| &w.stem).filter(|s| answer.get(s.as_str()) != Some(&false)));
    }
    tick.extend(a.answers.iter().filter(|x| x.known && rank.contains_key(x.stem.as_str())).map(|x| &x.stem));
    tick.sort();
    tick.dedup();
    match set_many(&state, user.0.id, &a.language, &tick, true).await {
        Ok(()) => Json(json!({ "estimate": estimate, "ticked": tick.len() })).into_response(),
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
        let c = compute(&text, "eo", None).unwrap();
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
        assert!(compute("some words here", "zh", None).is_none());
        assert!(compute("några ord här", "sv", None).is_some());
    }

    #[test]
    fn examples_are_whole_sentences_with_the_word_marked_and_spread_through_the_book() {
        let text = "Ella dijo que no. »¿Qué hizo el perro en la casa grande?» preguntó el hombre.\n".to_string()
            + &"Una frase sin la palabra que buscamos aquí hoy. ".repeat(30)
            + "El perro durmió toda la noche en la casa del pueblo. Perro! ";
        let forms = vec!["perro".to_string()];
        let found = examples_in(&text, &forms, 3);
        assert_eq!(found.len(), 2, "{found:?}");
        let joined: String = found[0].iter().map(|p| p.text.as_str()).collect();
        assert_eq!(joined, "»¿Qué hizo el perro en la casa grande?»");
        assert_eq!(found[0].iter().filter(|p| p.hit).map(|p| p.text.as_str()).collect::<Vec<_>>(), ["perro"]);
        assert!(found[1][1].hit && found[1][1].text == "perro", "{:?}", found[1]);
        // A paragraph with hard line breaks is read as running text.
        let wrapped = "Cuando me lo\ncontó le pregunté por qué el perro\nlloraba tanto. CAPÍTULO I\nEl perro de Augusto dormía en\nla cocina grande.";
        let found = examples_in(wrapped, &forms, 3);
        let texts: Vec<String> = found.iter().map(|p| p.iter().map(|p| p.text.as_str()).collect::<String>()).collect();
        assert_eq!(texts, ["Cuando me lo contó le pregunté por qué el perro lloraba tanto."], "{texts:?}");
        // A heading is not part of the sentence after it.
        let found = examples_in("Ahora\nLlueve sobre el perro de la mujer.\nNace Luna", &forms, 3);
        assert_eq!(found[0].iter().map(|p| p.text.as_str()).collect::<String>(), "Llueve sobre el perro de la mujer.");
        // Too short to be an example: "Perro!".
        assert!(examples_in("Perro! Perro.", &forms, 3).is_empty());
    }

    #[test]
    fn gutenbergs_licence_is_not_counted() {
        let text = "Header *** START OF THE PROJECT GUTENBERG EBOOK X *** the story *** END OF THE PROJECT GUTENBERG EBOOK X *** licence";
        assert_eq!(without_gutenberg(text).trim(), "the story");
    }
}


// ------------------------------------------------------------- the words page

async fn known_stems(state: &AppState, user: i64, language: &str) -> anyhow::Result<std::collections::HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT stem FROM known_words WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(language)
        .fetch_all(&state.db)
        .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

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
    out.sort_by(|a, b| (b.books + b.known).cmp(&(a.books + a.known)));
    Json(out).into_response()
}

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
    /// The reader's books it occurs in, and how often in all.
    pub books: Vec<i64>,
    pub count: u32,
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
    /// Words of the reader's books that match the search (`q`), with the
    /// known ones among them.
    pub matches: Vec<WordRow>,
    pub source: Option<String>,
    /// The language pack the meanings come from (none for Esperanto).
    pub pack: Option<crate::langpack::Meta>,
}

/// A word's English meaning and class, from the language's pack (by its key,
/// the stem) or ESPDIC (by the word).
fn meaning_of(state: &AppState, language: &str, word: &str, stem: &str) -> (Option<String>, Option<&'static str>) {
    if language == "eo" {
        (crate::espdic::lookup(word).map(|f| f.english), Some(esperanto_class(word)))
    } else {
        match crate::langpack::get(&state.data_dir, language).and_then(|p| {
            p.find(stem).map(|l| (Some(l.english.to_string()).filter(|e| !e.is_empty()), l.pos.split(',').next().map(class_of_pos)))
        }) {
            Some(found) => found,
            None => (None, None),
        }
    }
}

fn fill_meaning(state: &AppState, language: &str, row: &mut WordRow) {
    let (english, class) = meaning_of(state, language, &row.word, &row.stem);
    row.english = english;
    row.class = class;
}

/// The words of the reader's books in a language, names aside ($1 the
/// reader, $2 the language), for the sums below.
const READERS_WORDS: &str = "FROM vocabulary_words w
     JOIN books b ON b.file_sha256 = w.file_sha256 AND b.language = w.language
     WHERE b.owner_id = $1 AND w.language = $2 AND w.name = 0";

const IS_KNOWN: &str = "EXISTS (SELECT 1 FROM known_words k WHERE k.user_id = $1 AND k.language = $2 AND k.stem = w.stem)";

/// A word summed over the reader's books: (stem, word, uses, books).
type Total = (String, String, i64, i64);

fn row_of((stem, word, n, _): Total, known: bool) -> WordRow {
    WordRow { stem, word, english: None, class: None, known, books: Vec::new(), count: n.clamp(0, u32::MAX as i64) as u32 }
}

/// The reader's books each word occurs in, for a few words.
async fn books_of(state: &AppState, user: i64, language: &str, rows: &mut [WordRow]) -> anyhow::Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let marks: Vec<String> = (0..rows.len()).map(|i| format!("${}", i + 3)).collect();
    let sql = format!("SELECT w.stem, b.id {READERS_WORDS} AND w.stem IN ({}) ORDER BY b.id", marks.join(", "));
    let mut q = sqlx::query_as::<_, (String, i64)>(&sql).bind(user).bind(language);
    for r in rows.iter() {
        q = q.bind(&r.stem);
    }
    let mut found: HashMap<String, Vec<i64>> = HashMap::new();
    for (stem, id) in q.fetch_all(&state.db).await? {
        found.entry(stem).or_default().push(id);
    }
    for r in rows.iter_mut() {
        r.books = found.remove(&r.stem).unwrap_or_default();
    }
    Ok(())
}

/// The reader's books in a language with how much of each they know: those
/// they are reading first (last read first), then those they know most of.
async fn book_shares(state: &AppState, user: i64, language: &str) -> anyhow::Result<Vec<BookShare>> {
    let rows: Vec<(i64, String, Option<String>, i64, i64, i64)> = sqlx::query_as(
        "SELECT b.id, b.title, b.author, v.words,
             CAST(COALESCE(SUM(CASE WHEN w.name = 1 THEN w.n ELSE 0 END), 0) AS BIGINT),
             CAST(COALESCE(SUM(CASE WHEN w.name = 0 AND k.stem IS NOT NULL THEN w.n ELSE 0 END), 0) AS BIGINT)
         FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         LEFT JOIN vocabulary_words w ON w.file_sha256 = v.file_sha256 AND w.language = v.language
         LEFT JOIN known_words k ON k.user_id = $1 AND k.language = w.language AND k.stem = w.stem
         WHERE b.owner_id = $1 AND b.language = $2 AND v.study IS NOT NULL AND v.words > 0
         GROUP BY b.id, b.title, b.author, v.words",
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
        .map(|(id, title, author, words, names, known)| {
            let learnable = (words - names).max(1);
            BookShare { id, title, author, share: known as f64 / learnable as f64, reading: reading.contains_key(&id) }
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
/// next, how many words they know, and (with `q`) the words that match a
/// search.
#[derive(Deserialize)]
pub struct OverviewQuery {
    language: String,
    /// A search among all the words of the reader's books in the language.
    q: Option<String>,
}

pub async fn overview(State(state): State<AppState>, user: AuthUser, Query(q): Query<OverviewQuery>) -> Response {
    match overview_of(&state, user.0.id, q).await {
        Ok(o) => Json(o).into_response(),
        Err(e) => internal(e),
    }
}

async fn overview_of(state: &AppState, user: i64, q: OverviewQuery) -> anyhow::Result<Overview> {
    let search = q.q.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let language = q.language;
    let mut books = book_shares(state, user, &language).await?;
    let books_total = books.len();
    books.truncate(FIRST_BOOKS);
    // Words to learn next: in more books first, then more often.
    let next: Vec<Total> = sqlx::query_as(&format!(
        "SELECT w.stem, MIN(w.word), CAST(SUM(w.n) AS BIGINT), COUNT(*) {READERS_WORDS} AND NOT {IS_KNOWN}
         GROUP BY w.stem ORDER BY COUNT(*) DESC, SUM(w.n) DESC, MIN(w.word) LIMIT 30"
    ))
    .bind(user)
    .bind(&language)
    .fetch_all(&state.db)
    .await?;
    let mut next: Vec<WordRow> = next.into_iter().map(|t| row_of(t, false)).collect();
    let (known,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM known_words WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(&language)
        .fetch_one(&state.db)
        .await?;
    // How many of the commonest words the reader knows.
    let (common_known, common_of): (i64, i64) = sqlx::query_as(&format!(
        "SELECT COUNT(k.stem), COUNT(*) FROM
           (SELECT w.stem, SUM(w.n) AS total {READERS_WORDS} GROUP BY w.stem ORDER BY total DESC, w.stem LIMIT {COMMON_WORDS}) t
         LEFT JOIN known_words k ON k.user_id = $1 AND k.language = $2 AND k.stem = t.stem"
    ))
    .bind(user)
    .bind(&language)
    .fetch_one(&state.db)
    .await?;
    let mut matches = match &search {
        Some(s) => search_words(state, user, &language, s).await?,
        None => Vec::new(),
    };
    books_of(state, user, &language, &mut next).await?;
    books_of(state, user, &language, &mut matches).await?;
    for row in next.iter_mut().chain(matches.iter_mut()) {
        fill_meaning(state, &language, row);
    }
    let pack = if language == "eo" { None } else { crate::langpack::get(&state.data_dir, &language).map(|p| p.meta.clone()) };
    let source = if language == "eo" { Some(crate::espdic::ATTRIBUTION.to_string()) } else { pack.as_ref().map(|p| p.attribution.clone()) };
    Ok(Overview {
        language,
        books,
        books_total,
        next,
        known,
        common: CommonKnown { known: common_known, of: common_of },
        matches,
        source,
        pack,
    })
}

/// Words of the reader's books that match a search: by the word, one of the
/// forms the books write it in, or its English. Exact words first.
async fn search_words(state: &AppState, user: i64, language: &str, q: &str) -> anyhow::Result<Vec<WordRow>> {
    // Every word of the reader's books, with whether one of its forms
    // matches; the meaning is looked up here, not in the database.
    let like = format!("%{}%", q.replace('\\', "").replace('%', "").replace('_', ""));
    let all: Vec<(String, String, i64, i64, i64, i64)> = sqlx::query_as(&format!(
        "SELECT w.stem, MIN(w.word), CAST(SUM(w.n) AS BIGINT), COUNT(*),
             MAX(CASE WHEN w.forms LIKE $3 OR w.stem LIKE $3 THEN 1 ELSE 0 END),
             MAX(CASE WHEN {IS_KNOWN} THEN 1 ELSE 0 END)
         {READERS_WORDS} GROUP BY w.stem"
    ))
    .bind(user)
    .bind(language)
    .bind(&like)
    .fetch_all(&state.db)
    .await?;
    let mut found: Vec<WordRow> = all
        .into_iter()
        .filter(|(stem, word, _, _, form, _)| {
            *form == 1
                || word.to_lowercase().contains(q)
                || meaning_of(state, language, word, stem).0.is_some_and(|e| e.to_lowercase().contains(q))
        })
        .map(|(stem, word, n, books, _, known)| row_of((stem, word, n, books), known == 1))
        .collect();
    found.sort_by(|a, b| {
        let exact = |w: &WordRow| w.word.to_lowercase() != q;
        (exact(a), b.count).cmp(&(exact(b), a.count))
    });
    found.truncate(50);
    Ok(found)
}

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
    match book_shares(&state, user.0.id, &q.language).await {
        Ok(all) => {
            let total = all.len();
            let page: Vec<BookShare> = all.into_iter().skip(q.offset).take(q.limit.unwrap_or(100).min(500)).collect();
            Json(json!({ "total": total, "books": page })).into_response()
        }
        Err(e) => internal(e),
    }
}

/// GET /api/vocabulary/known/words?language=&q=&sort=&class=&offset=&limit=:
/// the words the reader knows, a page at a time, with their meanings and the
/// books they occur in. `sort` is `recent` (the default: last ticked first),
/// `common` (most often in the reader's books) or `alpha`; without `limit`
/// all of them come (for an export).
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
    match known_words_of(&state, user.0.id, q).await {
        Ok(k) => Json(k).into_response(),
        Err(e) => internal(e),
    }
}

async fn known_words_of(state: &AppState, user: i64, q: KnownWordsQuery) -> anyhow::Result<KnownWords> {
    let ticked: Vec<(String, Option<String>, Option<i64>, Option<i64>)> = sqlx::query_as(&format!(
        "SELECT k.stem, t.word, t.total, t.books FROM known_words k
         LEFT JOIN (SELECT w.stem, MIN(w.word) AS word, CAST(SUM(w.n) AS BIGINT) AS total, COUNT(*) AS books
                    {READERS_WORDS} GROUP BY w.stem) t ON t.stem = k.stem
         WHERE k.user_id = $1 AND k.language = $2
         ORDER BY k.created_at DESC, k.stem"
    ))
    .bind(user)
    .bind(&q.language)
    .fetch_all(&state.db)
    .await?;
    let mut rows: Vec<WordRow> = ticked
        .into_iter()
        .map(|(stem, word, n, books)| {
            let word = word.unwrap_or_else(|| stem.clone());
            row_of((stem, word, n.unwrap_or(0), books.unwrap_or(0)), true)
        })
        .collect();
    let filter = q.q.as_deref().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let class = q.class.as_deref().filter(|c| !c.is_empty() && *c != "all");
    // Meanings are looked up for every word only when the filter needs them.
    if filter.is_some() || class.is_some() {
        for row in rows.iter_mut() {
            fill_meaning(state, &q.language, row);
        }
        rows.retain(|w| {
            filter.as_ref().is_none_or(|f| w.word.to_lowercase().contains(f.as_str()) || w.english.as_ref().is_some_and(|e| e.to_lowercase().contains(f.as_str())))
                && class.is_none_or(|c| w.class.unwrap_or("other") == c)
        });
    }
    match q.sort.as_deref() {
        Some("common") => rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word))),
        Some("alpha") => rows.sort_by(|a, b| a.word.to_lowercase().cmp(&b.word.to_lowercase())),
        _ => {}
    }
    let total = rows.len();
    let limit = q.limit.unwrap_or(usize::MAX);
    let mut page: Vec<WordRow> = rows.into_iter().skip(q.offset).take(limit).collect();
    if filter.is_none() && class.is_none() {
        for row in page.iter_mut() {
            fill_meaning(state, &q.language, row);
        }
    }
    // The books of each word: in chunks, so that an export of thousands of
    // words stays within the database's limit of parameters.
    for chunk in page.chunks_mut(500) {
        books_of(state, user, &q.language, chunk).await?;
    }
    Ok(KnownWords { total, words: page })
}

// ------------------------------------------------------------ examples

/// Example sentences a word gets at most, and from how many books.
const EXAMPLES: usize = 3;
const EXAMPLE_BOOKS: usize = 3;
/// An example sentence is this many words long, so it shows how the word is
/// used without being a fragment or a page.
const EXAMPLE_WORDS: std::ops::RangeInclusive<usize> = 5..=35;
/// Sentences apart that two examples from one book are at least.
const EXAMPLE_SPREAD: usize = 25;

/// A piece of an example sentence: the word itself (`hit`) or the text
/// around it.
#[derive(Serialize, Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub hit: bool,
}

/// The sentences of a text: split after . ! ? …, with closing quotes kept
/// with their sentence. A line break ends a sentence too (a heading ends at
/// one) unless the next line starts with a small letter: then it only wraps
/// a sentence, as in a book with hard line breaks in its paragraphs.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let ends = match c {
            '\n' => !text[at + 1..].trim_start().chars().next().is_some_and(char::is_lowercase),
            '.' | '!' | '?' | '…' => chars.peek().is_none_or(|(_, n)| n.is_whitespace() || matches!(n, '"' | '”' | '»' | '\'' | '’' | ')')),
            _ => false,
        };
        if !ends {
            continue;
        }
        let mut end = at + c.len_utf8();
        while let Some(&(i, n)) = chars.peek() {
            if matches!(n, '"' | '”' | '»' | '\'' | '’' | ')') {
                end = i + n.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        let s = text[start..end].trim();
        if !s.is_empty() {
            out.push(s);
        }
        start = end;
    }
    let s = text[start..].trim();
    if !s.is_empty() {
        out.push(s);
    }
    out
}

/// Up to `max` sentences of a book's text with one of the forms in it,
/// in the order of the book and spread through it, cut into pieces with the
/// forms marked.
fn examples_in(text: &str, forms: &[String], max: usize) -> Vec<Vec<Piece>> {
    let forms: Vec<String> = forms.iter().map(|f| f.to_lowercase()).collect();
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    let mut seen: Vec<&str> = Vec::new();
    for (n, sentence) in sentences(without_gutenberg(text)).into_iter().enumerate() {
        if out.len() >= max {
            break;
        }
        if last.is_some_and(|l| n < l + EXAMPLE_SPREAD) {
            continue;
        }
        // One starting with a small letter is the end of a sentence cut off,
        // one without a full stop or the like a heading or a line cut off.
        let ends = sentence.trim_end_matches(['"', '”', '»', '\'', '’', ')']).ends_with(['.', '!', '?', '…']);
        if !ends || sentence.chars().find(|c| c.is_alphabetic()).is_none_or(char::is_lowercase) {
            continue;
        }
        let tokens = words(sentence);
        if !EXAMPLE_WORDS.contains(&tokens.len()) || !tokens.iter().any(|t| forms.contains(&t.to_lowercase())) || seen.contains(&sentence) {
            continue;
        }
        seen.push(sentence);
        let mut pieces = Vec::new();
        let mut at = 0;
        for t in tokens {
            if !forms.contains(&t.to_lowercase()) {
                continue;
            }
            // A token is a slice of the sentence: its place is its address.
            let start = t.as_ptr() as usize - sentence.as_ptr() as usize;
            if start > at {
                pieces.push(Piece { text: one_line(&sentence[at..start]), hit: false });
            }
            pieces.push(Piece { text: t.to_string(), hit: true });
            at = start + t.len();
        }
        if at < sentence.len() {
            pieces.push(Piece { text: one_line(&sentence[at..]), hit: false });
        }
        out.push(pieces);
        last = Some(n);
    }
    out
}

/// Text with its runs of white space (line breaks too) as single spaces.
fn one_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

#[derive(Deserialize)]
pub struct ExamplesQuery {
    language: String,
    stem: String,
}

#[derive(Serialize)]
pub struct Example {
    pub book: i64,
    pub title: String,
    pub pieces: Vec<Piece>,
}

/// GET /api/vocabulary/examples?language=&stem=: a few sentences with the
/// word from the reader's books in the language, from the books that use it
/// most. Asked for only when the reader opens them, since a sentence from
/// later in a book can give away what happens.
pub async fn examples(State(state): State<AppState>, user: AuthUser, Query(q): Query<ExamplesQuery>) -> Response {
    // The books with the word, most uses first, and the forms each writes it in.
    let found: Vec<(i64, String, String, i64, String)> = match sqlx::query_as(&format!(
        "SELECT b.id, b.title, b.uuid, w.n, w.forms {READERS_WORDS} AND w.stem = $3 ORDER BY w.n DESC, b.id LIMIT {EXAMPLE_BOOKS}"
    ))
    .bind(user.0.id)
    .bind(&q.language)
    .bind(&q.stem)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let mut per_book: Vec<Vec<Example>> = Vec::new();
    for (id, title, uuid, _, forms) in found {
        let forms: Vec<String> = forms.split(' ').filter(|f| !f.is_empty()).map(String::from).collect();
        let path = crate::books::book_path(&state, &uuid, Format::Epub);
        state.storage.fetch(&path).await;
        let sentences = tokio::task::spawn_blocking(move || -> Vec<Vec<Piece>> {
            let Ok(sections) = booktext::read_sections(Format::Epub, &path) else { return Vec::new() };
            let text: String = sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
            examples_in(&text, &forms, EXAMPLES)
        })
        .await
        .unwrap_or_default();
        per_book.push(sentences.into_iter().map(|pieces| Example { book: id, title: title.clone(), pieces }).collect());
    }
    give_memory_back();
    // One from each book in turn, so that several books are heard.
    let mut out = Vec::new();
    let mut iters: Vec<_> = per_book.into_iter().map(|v| v.into_iter()).collect();
    while out.len() < EXAMPLES {
        let before = out.len();
        for it in iters.iter_mut() {
            if out.len() < EXAMPLES {
                out.extend(it.next());
            }
        }
        if out.len() == before {
            break;
        }
    }
    Json(json!({ "examples": out })).into_response()
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
}

/// GET /api/books/{id}/vocabulary/word?w=: a word selected in the reader,
/// as the book's study list has it: its dictionary form, meaning, class and
/// whether the reader knows it. 404 when the book has no study list or the
/// word is a name.
pub async fn word_in_book(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>, Query(q): Query<WordQuery>) -> Response {
    let not_found = || (StatusCode::NOT_FOUND, Json(json!({ "error": "not on the study list" }))).into_response();
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
    let Some((language, json)) = row else { return not_found() };
    let list: Vec<StudyWord> = serde_json::from_str(&json).unwrap_or_default();
    let written: String = q.w.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let written = if language == "eo" { crate::esperanto::supersigned(&written).unwrap_or(written) } else { written };
    if written.is_empty() || written.chars().count() > 60 {
        return not_found();
    }
    // As the list has it: by one of its forms, else by the word's stem
    // (Esperanto) or lemma (a pack) when the form is not among those kept.
    let pack = if language == "eo" { None } else { crate::langpack::get(&state.data_dir, &language) };
    let key = match &pack {
        Some(p) => {
            let lemmas = p.lemmas_of(&written);
            lemmas
                .iter()
                .filter_map(|l| p.lemma(*l).map(|x| x.word.to_string()))
                .find(|w| list.iter().any(|s| &s.stem == w))
                .or_else(|| lemmas.first().and_then(|l| p.lemma(*l)).map(|x| x.word.to_string()))
        }
        None => Some(esperanto_stem(&written)),
    };
    let found = list
        .iter()
        .find(|s| s.forms.iter().any(|f| *f == written))
        .or_else(|| key.as_ref().and_then(|k| list.iter().find(|s| &s.stem == k)));
    let (stem, word, count, name) = match (found, &key) {
        (Some(s), _) => (s.stem.clone(), s.word.clone(), s.count, s.name),
        (None, Some(k)) => {
            let shown = match &pack {
                Some(p) => p.find(k).map(|l| l.written.to_string()).unwrap_or_else(|| k.clone()),
                None => crate::espdic::lookup(&written).map(|f| f.headword).unwrap_or_else(|| written.clone()),
            };
            (k.clone(), shown, 0, false)
        }
        (None, None) => return not_found(),
    };
    if name {
        return not_found();
    }
    let (english, class) = match &pack {
        Some(p) => {
            let l = p.find(&stem);
            (l.as_ref().map(|l| l.english.to_string()).filter(|e| !e.is_empty()), l.and_then(|l| l.pos.split(',').next().map(class_of_pos)))
        }
        None => (crate::espdic::lookup(&word).map(|f| f.english), Some(esperanto_class(&word))),
    };
    let known = match known_stems(&state, user.0.id, &language).await {
        Ok(k) => k.contains(&stem),
        Err(e) => return internal(e),
    };
    Json(WordInBook { stem, word, english, class, known, count }).into_response()
}
