//! A book's vocabulary, after Zipf: a few words make up most of any text.
//! For each EPUB (once per file and language) a worker counts the words,
//! takes each to its stem (Snowball; Esperanto by its regular endings; a
//! language pack's lemmas for a language without either), and keeps how
//! many of the commonest stems cover half, 80, 90 and 95 % of the text, plus
//! the stems that may characterise the book. Comparisons with the other
//! books in the same language on the instance are made when asked, since
//! they change as books come and go. A book in a language with a language
//! pack (langpack.rs), or in Esperanto with ESPDIC, also gets a study list:
//! its words in order of frequency with their English, which the reader
//! ticks off as known.
//!
//! - `count`: from a text to the numbers and the study list;
//! - `index`: the worker that counts books and keeps the results;
//! - `book`: one book's vocabulary and study list, and the known words;
//! - `words`: the words page, over all the reader's books in a language,
//!   read from a summary per reader and language that is checked and, when
//!   out of date, built again as the page is read;
//! - `level`: the level test;
//! - `examples`: example sentences from the reader's books.

mod book;
mod count;
mod examples;
mod index;
mod level;
mod words;

pub use book::{for_book, known_list, known_many, known_set, turned_on, word_in_book};
pub use count::Stemmer;
pub(crate) use book::stopwords;
pub(crate) use count::{compute, without_gutenberg, words};
pub(crate) use examples::{one_line, sentences, stored_text};
pub(crate) use index::give_memory_back;
pub use examples::examples;
pub use index::worker;
#[cfg(test)]
pub(crate) use index::{count_pending, drop_orphans, index_pending};
pub use level::{level_apply, level_sample};
pub use words::{books, known_words, languages, overview, search};

use crate::langpack::Pack;
use crate::AppState;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

/// Raised when what is computed changes, so that every book is done again.
const VERSION: i64 = 8;

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

/// The longest language code and search a request may carry.
const LANGUAGE_MAX: usize = 12;
const QUERY_MAX: usize = 80;

/// The language's pack, for counting and meanings alike: none for Esperanto,
/// which has ESPDIC (a change there rides on VERSION).
pub(crate) fn pack_for(state: &AppState, language: &str) -> Option<Arc<Pack>> {
    if language == "eo" {
        None
    } else {
        state.packs.get(language)
    }
}

/// What `vocabulary.pack` holds for a book counted with this pack: its id,
/// or '' without one.
fn pack_id(pack: Option<&Arc<Pack>>) -> String {
    pack.map(|p| p.id().to_string()).unwrap_or_default()
}

// ------------------------------------------------------------ what is kept

/// A stem that may characterise the book: common here, perhaps rare elsewhere.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Candidate {
    pub stem: String,
    pub label: String,
    pub count: u32,
    pub name: bool,
}

/// A word of a study list: the key known words are kept by, the dictionary
/// form shown, the forms the book writes it in (most common first).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StudyWord {
    pub stem: String,
    pub word: String,
    pub forms: Vec<String>,
    pub count: u32,
    pub name: bool,
}

/// What counting a text gives.
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

// --------------------------------------------------------------- meanings

/// A word's English and class as the glossary has it.
#[derive(Default)]
pub(crate) struct Meaning {
    pub english: Option<String>,
    /// The headword the word was found under when it had to be taken apart
    /// (ESPDIC).
    pub found_as: Option<String>,
    /// "verb", "noun", "adj" or "other" (little words, adverbs and the rest);
    /// none for a word without a known class.
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

/// Where a language's meanings come from: ESPDIC for Esperanto, the
/// language's pack otherwise, or nowhere.
pub(crate) enum Glossary {
    Espdic,
    Pack(Arc<Pack>),
    None,
}

impl Glossary {
    pub fn for_language(state: &AppState, language: &str) -> Glossary {
        if language == "eo" {
            return Glossary::Espdic;
        }
        match pack_for(state, language) {
            Some(p) => Glossary::Pack(p),
            None => Glossary::None,
        }
    }

    /// A study word's meaning: by its key (the lemma) in a pack, by the
    /// word in ESPDIC.
    pub fn meaning(&self, word: &str, stem: &str) -> Meaning {
        match self {
            Glossary::Espdic => {
                let found = crate::espdic::lookup(word);
                Meaning {
                    english: found.as_ref().map(|f| f.english.clone()),
                    found_as: found.filter(|f| f.derived).map(|f| f.headword),
                    class: Some(crate::esperanto::class(word)),
                }
            }
            Glossary::Pack(p) => match p.find(stem) {
                Some(l) => Meaning {
                    english: Some(l.english.to_string()).filter(|e| !e.is_empty()),
                    found_as: None,
                    class: l.pos.split(',').next().map(class_of_pos),
                },
                None => Meaning::default(),
            },
            Glossary::None => Meaning::default(),
        }
    }

    pub fn pack(&self) -> Option<&Arc<Pack>> {
        match self {
            Glossary::Pack(p) => Some(p),
            _ => None,
        }
    }

    /// Who to name wherever a meaning from here is shown.
    pub fn attribution(&self) -> Option<String> {
        match self {
            Glossary::Espdic => Some(crate::espdic::ATTRIBUTION.to_string()),
            Glossary::Pack(p) => Some(p.meta.attribution.clone()),
            Glossary::None => None,
        }
    }
}
