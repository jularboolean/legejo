//! The concordance: every place in a book where a word or a phrase stands,
//! each with the text before and after it, so that the lines can be read
//! under each other. A single word is found in all its forms (by its stem,
//! or by its lemma with a language pack) unless the exact form is asked for.

use super::{bad_request, book, file_text, internal};
use crate::auth::AuthUser;
use crate::booktext::floor_boundary;
use crate::langpack::Pack;
use crate::vocabulary::{give_memory_back, one_line, pack_for, stored_text, without_gutenberg, words, Stemmer};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Lines one answer carries at most; the count is of all there are.
const LINES: usize = 300;
/// Bytes of text shown before and after the word.
const CONTEXT: usize = 70;
/// The longest word or phrase, in letters and in words.
const QUERY_MAX: usize = 80;
const QUERY_WORDS: usize = 6;
/// Forms of the word listed with their counts.
const FORMS: usize = 12;

#[derive(Serialize, Debug)]
pub struct Line {
    pub left: String,
    pub hit: String,
    pub right: String,
    /// Where in the book, 0 to 1.
    pub at: f64,
}

#[derive(Serialize, Debug, Default)]
pub struct Concordance {
    pub total: usize,
    pub lines: Vec<Line>,
    /// The forms found, most common first, with their counts.
    pub forms: Vec<(String, u32)>,
}

/// Whether two lower-case words are forms of the same word.
struct Same<'a> {
    stemmer: Option<&'a Stemmer>,
    pack: Option<&'a Pack>,
    stem: Option<String>,
    lemmas: Vec<u32>,
}

impl<'a> Same<'a> {
    fn new(word: &str, stemmer: Option<&'a Stemmer>, pack: Option<&'a Pack>) -> Same<'a> {
        Same { stem: stemmer.map(|s| s.stem(word)), lemmas: pack.map(|p| p.lemmas_of(word)).unwrap_or_default(), stemmer, pack }
    }

    fn is(&self, other: &str) -> bool {
        self.stemmer.zip(self.stem.as_ref()).is_some_and(|(s, stem)| &s.stem(other) == stem)
            || (!self.lemmas.is_empty() && self.pack.is_some_and(|p| p.lemmas_of(other).iter().any(|l| self.lemmas.contains(l))))
    }
}

/// The text before a place, cut at a word, on one line.
fn before(text: &str, at: usize) -> String {
    let from = floor_boundary(text, at.saturating_sub(CONTEXT));
    let piece = &text[from..at];
    // Not from the middle of a word.
    let piece = if from > 0 { piece.find(char::is_whitespace).map(|i| &piece[i..]).unwrap_or(piece) } else { piece };
    one_line(piece).trim().to_string()
}

/// The text after a place, cut at a word, on one line.
fn after(text: &str, at: usize) -> String {
    let to = floor_boundary(text, (at + CONTEXT).min(text.len()));
    let piece = &text[at..to];
    let piece = if to < text.len() { piece.rfind(char::is_whitespace).map(|i| &piece[..i]).unwrap_or(piece) } else { piece };
    one_line(piece).trim().to_string()
}

pub(super) fn find(text: &str, query: &str, exact: bool, stemmer: Option<&Stemmer>, pack: Option<&Pack>) -> Concordance {
    let text = without_gutenberg(text);
    let wanted: Vec<String> = words(query).iter().map(|w| w.to_lowercase()).collect();
    if wanted.is_empty() || wanted.len() > QUERY_WORDS {
        return Concordance::default();
    }
    let tokens = words(text);
    let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    // One word in all its forms; a phrase, or a word asked for exactly, as written.
    let same = (wanted.len() == 1 && !exact).then(|| Same::new(&wanted[0], stemmer, pack));
    let mut is_form: HashMap<&str, bool> = HashMap::new();
    let base = text.as_ptr() as usize;
    let mut out = Concordance::default();
    let mut forms: HashMap<String, u32> = HashMap::new();
    let mut i = 0;
    while i + wanted.len() <= tokens.len() {
        let hit = match &same {
            Some(same) => lower[i] == wanted[0] || *is_form.entry(lower[i].as_str()).or_insert_with(|| same.is(&lower[i])),
            None => lower[i..i + wanted.len()] == wanted[..],
        };
        if !hit {
            i += 1;
            continue;
        }
        let last = tokens[i + wanted.len() - 1];
        let start = tokens[i].as_ptr() as usize - base;
        let end = last.as_ptr() as usize - base + last.len();
        out.total += 1;
        *forms.entry(lower[i..i + wanted.len()].join(" ")).or_default() += 1;
        if out.lines.len() < LINES {
            out.lines.push(Line {
                left: before(text, start),
                hit: one_line(&text[start..end]),
                right: after(text, end),
                at: (start as f64 / text.len().max(1) as f64 * 1000.0).round() / 1000.0,
            });
        }
        i += wanted.len();
    }
    let mut forms: Vec<(String, u32)> = forms.into_iter().collect();
    forms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    forms.truncate(FORMS);
    out.forms = forms;
    out
}

#[derive(Deserialize)]
pub struct ConcordanceQuery {
    q: String,
    /// Only the form as written, not the word's other forms.
    #[serde(default)]
    exact: bool,
}

/// GET /api/books/{id}/analysis/concordance?q=&exact=: the lines of the book
/// with the word or phrase. Read from the text the search inside the books
/// keeps when it has the book, else from the file.
pub async fn concordance(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>, Query(q): Query<ConcordanceQuery>) -> Response {
    let query = q.q.trim().to_string();
    if query.is_empty() || query.chars().count() > QUERY_MAX {
        return bad_request("a word or a phrase");
    }
    let book = match book(&state, user.0.id, id).await {
        Ok(b) => b,
        Err(r) => return r,
    };
    let kept = match &book.text_hash {
        Some(hash) => stored_text(&state, hash).await,
        None => None,
    };
    let text = match kept {
        Some(text) => text,
        None => match file_text(&state, &book.uuid).await {
            Ok(text) => text,
            Err(e) => return internal(e),
        },
    };
    let pack: Option<Arc<Pack>> = pack_for(&state, &book.language);
    let language = book.language.clone();
    let found = tokio::task::spawn_blocking(move || {
        let stemmer = Stemmer::for_book(&language, pack.clone());
        find(&text, &query, q.exact, stemmer.as_ref(), pack.as_deref())
    })
    .await;
    give_memory_back();
    match found {
        Ok(found) => Json(found).into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_found_in_its_forms_with_the_text_around_it() {
        let text = "La hundo dormis. Poste Petro vidis la grandan hundon en la ĝardeno de la domo, kaj du hundoj bojis.\nLa kato ne vidis ĝin.";
        let stemmer = Stemmer::for_language("eo");
        let found = find(text, "hundo", false, stemmer.as_ref(), None);
        assert_eq!(found.total, 3);
        assert_eq!(found.lines.iter().map(|l| l.hit.as_str()).collect::<Vec<_>>(), ["hundo", "hundon", "hundoj"]);
        assert_eq!(found.lines[0].left, "La");
        assert!(found.lines[1].left.ends_with("vidis la grandan"), "{}", found.lines[1].left);
        assert!(found.lines[1].right.starts_with("en la ĝardeno"), "{}", found.lines[1].right);
        assert!(found.lines[2].right.ends_with("La kato ne vidis ĝin."), "the line break is a space: {}", found.lines[2].right);
        assert!(found.lines[0].at < found.lines[2].at);
        assert_eq!(found.forms, [("hundo".to_string(), 1), ("hundoj".to_string(), 1), ("hundon".to_string(), 1)]);
        // The exact form alone.
        assert_eq!(find(text, "hundo", true, stemmer.as_ref(), None).total, 1);
        // A phrase as written, letter case aside.
        let found = find(text, "LA GRANDAN hundon", false, stemmer.as_ref(), None);
        assert_eq!((found.total, found.lines[0].hit.as_str()), (1, "la grandan hundon"));
        assert_eq!(find(text, "kato hundo", false, stemmer.as_ref(), None).total, 0);
        assert_eq!(find(text, "…", false, stemmer.as_ref(), None).total, 0);
    }

    #[test]
    fn a_pack_finds_the_forms_a_stemmer_does_not() {
        let es = crate::langpack::fixture("es");
        let stemmer = Stemmer::for_book("es", Some(es.clone()));
        let text = "Ayer hizo frío. Hoy hace calor y mañana hará viento, dijo la mujer que hacía pan.";
        let found = find(text, "hacer", false, stemmer.as_ref(), Some(&es));
        let hits: Vec<&str> = found.lines.iter().map(|l| l.hit.as_str()).collect();
        assert_eq!(hits, ["hizo", "hace", "hará", "hacía"]);
    }

    #[test]
    fn long_context_is_cut_at_a_word() {
        let text = format!("{} palabra {}", "antes ".repeat(40), "después ".repeat(40));
        let found = find(&text, "palabra", true, None, None);
        let line = &found.lines[0];
        assert!(line.left.len() <= CONTEXT && line.left.starts_with("antes") && line.left.ends_with("antes"), "{}", line.left);
        assert!(line.right.len() <= CONTEXT && line.right.starts_with("después") && line.right.ends_with("después"), "{}", line.right);
    }
}
