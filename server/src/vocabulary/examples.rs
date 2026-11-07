//! Example sentences: a few sentences with a word from the reader's books in
//! a language, read from the books that use it most: from the text the
//! search inside the books keeps (`book_text`) when it has the book, else
//! from the file.

use super::count::{without_gutenberg, words};
use super::index::give_memory_back;
use super::internal;
use super::words::summary;
use crate::auth::AuthUser;
use crate::booktext;
use crate::formats::Format;
use crate::AppState;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

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

/// A book's text as the search inside the books keeps it (fulltext.rs): its
/// parts in order, a line break between them, as the file's sections would
/// read. None when the text is not there (the search off, or the book not
/// read yet).
async fn stored_text(state: &AppState, text_hash: &str) -> Option<String> {
    let parts: Vec<(String,)> = sqlx::query_as("SELECT body FROM book_text WHERE text_hash = $1 ORDER BY section, pos")
        .bind(text_hash)
        .fetch_all(&state.db)
        .await
        .ok()?;
    if parts.is_empty() {
        return None;
    }
    Some(parts.iter().map(|(body,)| body.as_str()).collect::<Vec<_>>().join("\n"))
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
    if let Err(e) = summary(&state, user.0.id, &q.language).await {
        return internal(e);
    }
    // The books with the word, most uses first, and the forms each writes
    // it in: by the word's rows, then the reader's books among those files.
    let found: Vec<(i64, String, String, Option<String>, i64, String)> = match sqlx::query_as(&format!(
        "SELECT b.id, b.title, b.uuid, b.text_hash, w.n, w.forms FROM vocabulary_words w CROSS JOIN books b
         WHERE w.language = $2 AND w.stem = $3 AND w.name = 0
           AND b.file_sha256 = w.file_sha256 AND b.language = w.language AND b.owner_id = $1
         ORDER BY w.n DESC, b.id LIMIT {EXAMPLE_BOOKS}"
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
    for (id, title, uuid, text_hash, _, forms) in found {
        let forms: Vec<String> = forms.split(' ').filter(|f| !f.is_empty()).map(String::from).collect();
        let text = match text_hash {
            Some(hash) => stored_text(&state, &hash).await,
            None => None,
        };
        let sentences = match text {
            Some(text) => tokio::task::spawn_blocking(move || examples_in(&text, &forms, EXAMPLES)).await.unwrap_or_default(),
            None => {
                let path = crate::books::book_path(&state, &uuid, Format::Epub);
                state.storage.fetch(&path).await;
                tokio::task::spawn_blocking(move || -> Vec<Vec<Piece>> {
                    let Ok(sections) = booktext::read_sections(Format::Epub, &path) else { return Vec::new() };
                    let text: String = sections.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("\n");
                    examples_in(&text, &forms, EXAMPLES)
                })
                .await
                .unwrap_or_default()
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
