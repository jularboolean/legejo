//! A book's text made ready for the analysis: its sentences as runs of
//! words, each word numbered by its lower-case spelling, and for every word
//! whether it is spoken (inside quotation marks or on a line of dialogue).

use crate::vocabulary::{sentences, without_gutenberg, words};
use std::collections::HashMap;

/// A line without a full stop counts as a sentence from this many words: a
/// line of verse does, a heading does not.
const FRAGMENT_WORDS: usize = 6;

pub(super) struct Prepared<'a> {
    /// The words of the counted sentences, as written.
    pub tokens: Vec<&'a str>,
    /// Each token's word: its number among the distinct lower-case words.
    pub ids: Vec<u32>,
    /// The distinct words in lower case, by number.
    pub lower: Vec<String>,
    /// The sentences as token ranges, start and end.
    pub sentences: Vec<(u32, u32)>,
    /// Per token: whether it is spoken.
    pub spoken: Vec<bool>,
    /// Per token: whether it stands first in its sentence.
    pub first: Vec<bool>,
}

impl Prepared<'_> {
    pub fn word(&self, id: u32) -> &str {
        &self.lower[id as usize]
    }

    /// How often each distinct word occurs.
    pub fn counts(&self) -> Vec<u32> {
        let mut counts = vec![0u32; self.lower.len()];
        for id in &self.ids {
            counts[*id as usize] += 1;
        }
        counts
    }

    /// The token ranges of `n` parts of the book with about as many words
    /// each, cut between sentences.
    pub fn parts(&self, n: usize) -> Vec<(u32, u32)> {
        let total = self.tokens.len();
        let mut out = Vec::with_capacity(n);
        let mut start = 0u32;
        let mut sentence = 0usize;
        for part in 1..=n {
            let target = (total * part / n) as u32;
            let mut end = start;
            while sentence < self.sentences.len() && (self.sentences[sentence].1 <= target || end == start) {
                end = self.sentences[sentence].1;
                sentence += 1;
            }
            if part == n {
                end = total as u32;
            }
            out.push((start, end));
            start = end;
        }
        out
    }
}

const CLOSERS: [char; 6] = ['"', '”', '»', '\'', '’', ')'];

/// Titles written with a full stop that does not end the sentence ("Mr.
/// Bennet", "la Sra. Pérez"); a lone letter is an initial ("J. Austen").
const TITLES: [&str; 26] = [
    "mr", "mrs", "ms", "dr", "st", "prof", "rev", "col", "gen", "capt", "lt", "messrs", "sr", "sra", "srta", "ud", "uds", "vd", "mme", "mlle",
    "mgr", "hr", "fr", "frl", "hrn", "sro",
];

/// Whether a sentence only seems to end: at a title or an initial.
fn ends_in_title(sentence: &str, last: &str) -> bool {
    sentence.ends_with('.') && (TITLES.contains(&last.to_lowercase().as_str()) || (last.chars().count() == 1 && last.chars().all(char::is_uppercase)))
}

pub(super) fn prepare(text: &str) -> Prepared<'_> {
    let text = without_gutenberg(text);
    let spoken = spoken_ranges(text);
    let base = text.as_ptr() as usize;
    let mut p = Prepared { tokens: Vec::new(), ids: Vec::new(), lower: Vec::new(), sentences: Vec::new(), spoken: Vec::new(), first: Vec::new() };
    let mut word_ids: HashMap<String, u32> = HashMap::new();
    let mut lower = String::new();
    // Whether the sentence before was counted, so that what follows a full
    // stop with a small letter ("etc. and so on", "“No!” said he") joins it.
    let mut open = false;
    let mut title = false;
    for sentence in sentences(text) {
        let found = words(sentence);
        if found.is_empty() {
            continue;
        }
        let joins = open && (title || sentence.chars().find(|c| c.is_alphabetic()).is_some_and(char::is_lowercase));
        title = ends_in_title(sentence, found[found.len() - 1]);
        let ends = sentence.trim_end_matches(CLOSERS).ends_with(['.', '!', '?', '…']);
        if !joins && !ends && found.len() < FRAGMENT_WORDS {
            // A heading or a label.
            open = false;
            continue;
        }
        let start = p.tokens.len() as u32;
        for (i, token) in found.into_iter().enumerate() {
            lower.clear();
            lower.extend(token.chars().flat_map(char::to_lowercase));
            let id = match word_ids.get(lower.as_str()) {
                Some(&id) => id,
                None => {
                    let id = p.lower.len() as u32;
                    word_ids.insert(lower.clone(), id);
                    p.lower.push(lower.clone());
                    id
                }
            };
            let at = token.as_ptr() as usize - base;
            let n = spoken.partition_point(|(_, end)| *end <= at);
            p.spoken.push(spoken.get(n).is_some_and(|(from, _)| *from <= at));
            p.first.push(i == 0 && !joins);
            p.ids.push(id);
            p.tokens.push(token);
        }
        let end = p.tokens.len() as u32;
        match p.sentences.last_mut() {
            Some(last) if joins => last.1 = end,
            _ => p.sentences.push((start, end)),
        }
        open = true;
    }
    p
}

/// What closes a quotation opened by `open`. Swedish and Finnish open and
/// close with the same mark (”…”, »…»), German closes „ with “ and » with «.
fn closes(open: char, c: char) -> bool {
    match open {
        '“' => c == '”',
        '«' => c == '»',
        '„' => c == '“' || c == '”',
        '"' => c == '"',
        '”' => c == '”',
        '»' => c == '»' || c == '«',
        '‹' => c == '›',
        _ => false,
    }
}

/// The byte ranges of a text that are spoken: what stands inside quotation
/// marks, and the lines that begin with a dash, as dialogue is set in
/// Spanish, French, Swedish and Esperanto books. A quotation ends with its
/// paragraph at the latest; a line that begins with a small letter only
/// wraps the one before and carries on what was open there.
fn spoken_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut dash = false;
    let mut quote: Option<char> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        let trimmed = line.trim_start();
        let wraps = trimmed.chars().next().is_some_and(char::is_lowercase);
        if !wraps {
            quote = None;
            dash = trimmed.starts_with(['—', '–', '―']) || trimmed.starts_with("- ");
        }
        if dash {
            out.push((start, at));
            continue;
        }
        let mut from = quote.map(|_| start);
        for (i, c) in line.char_indices() {
            match quote {
                None if matches!(c, '“' | '«' | '„' | '"' | '”' | '»' | '‹') => {
                    quote = Some(c);
                    from = Some(start + i);
                }
                Some(open) if closes(open, c) => {
                    if let Some(f) = from.take() {
                        out.push((f, start + i));
                    }
                    quote = None;
                }
                _ => {}
            }
        }
        if let Some(f) = from {
            out.push((f, at));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken_words(text: &str) -> Vec<&str> {
        let p = prepare(text);
        p.tokens.iter().zip(&p.spoken).filter(|(_, s)| **s).map(|(t, _)| *t).collect()
    }

    #[test]
    fn sentences_are_counted_and_headings_are_not() {
        let p = prepare("Chapter One\nThe dog sleeps. It was late, e.g. after ten. “No!” said the man.\nRoses are red and violets are blue\nThen Mr. J. Bennet spoke to Tom. He said no.\n");
        let lens: Vec<u32> = p.sentences.iter().map(|(a, b)| b - a).collect();
        // "Chapter One" is a heading; "after ten" and "said the man" join the
        // sentence before; the line of verse counts without a full stop.
        // A title and an initial do not end theirs; a name does.
        assert_eq!(lens, [3, 7, 4, 7, 7, 3], "{:?}", p.tokens);
        assert!(p.first[0] && !p.first[1]);
        let said = p.tokens.iter().position(|t| *t == "said").unwrap();
        assert!(!p.first[said]);
        assert_eq!(p.word(p.ids[0]), "the");
        assert_eq!(p.ids[0], p.ids[p.tokens.iter().rposition(|t| *t == "the").unwrap()]);
    }

    #[test]
    fn spoken_words_are_inside_quotation_marks_or_on_a_line_with_a_dash() {
        assert_eq!(spoken_words("He came in. “Good evening,” he said, “is it late?” Nobody answered him."), ["Good", "evening", "is", "it", "late"]);
        assert_eq!(spoken_words("Hon log. ”Kom nu”, sade hon till honom.\n»Ja», svarade han efter en stund.\n"), ["Kom", "nu", "Ja"]);
        assert_eq!(spoken_words("Er kam. „Guten Abend“, sagte er zu ihr.\n»Schon gut«, sagte sie zu ihm.\n"), ["Guten", "Abend", "Schon", "gut"]);
        // A line of dialogue with a dash is spoken to its end, also when it wraps.
        let text = "Augusto salió de su casa.\n—¿Qué haces aquí? —preguntó\nel hombre.\nNadie contestó a la pregunta.\n";
        assert_eq!(spoken_words(text), ["Qué", "haces", "aquí", "preguntó", "el", "hombre"]);
        // A quotation left open ends with its paragraph.
        assert_eq!(spoken_words("«Venu al mi, li diris al ŝi.\nŜi ne venis al li tiam.\n"), ["Venu", "al", "mi", "li", "diris", "al", "ŝi"]);
    }

    #[test]
    fn parts_are_cut_between_sentences() {
        let text = "One two three four. ".repeat(10);
        let p = prepare(&text);
        let parts = p.parts(4);
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].0, 0);
        assert_eq!(parts[3].1, 40);
        for (a, b) in &parts {
            assert_eq!((b - a) % 4, 0, "{parts:?}");
            assert!(b > a);
        }
    }
}
