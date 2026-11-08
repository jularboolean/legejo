//! From a text to its numbers and its study list: the words as written,
//! each taken to its stem, the coverage curve, the candidate words, and the
//! study list by lemma where the language has a pack.

use super::{Candidate, Computed, StudyWord};
use crate::esperanto;
use crate::langpack::Pack;
use std::collections::HashMap;
use std::sync::Arc;

/// The fixed-size sample the difficulty is measured on, so that a long book
/// does not look harder only for being long.
const SAMPLE: usize = 20_000;
/// Stems kept as candidates for "characteristic": at least this often.
const CANDIDATE_MIN: u32 = 5;
const CANDIDATES: usize = 2000;
/// Words on a study list.
const STUDY: usize = 1500;

// ---------------------------------------------------------------- stemming

/// A language's stemmer: Snowball's, Esperanto's own endings, or, for a
/// language with neither, the lemmas of its pack.
pub enum Stemmer {
    Snowball(rust_stemmers::Stemmer),
    Esperanto,
    Pack(Arc<Pack>),
}

impl Stemmer {
    /// The stemmer of a language that has one of its own (searching uses
    /// these for prefix stems).
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

    /// The stemmer a book in a language is counted with: the language's
    /// own, else its pack's lemmas; None when it has neither (then the book
    /// has no vocabulary).
    pub fn for_book(code: &str, pack: Option<Arc<Pack>>) -> Option<Stemmer> {
        Stemmer::for_language(code).or_else(|| pack.map(Stemmer::Pack))
    }

    /// A lower-case word's stem.
    pub fn stem(&self, word: &str) -> String {
        match self {
            Stemmer::Snowball(s) => s.stem(word).into_owned(),
            Stemmer::Esperanto => esperanto::stem(word),
            // A form of several lemmas: the one spelt like it, else a content
            // word over a little one and the longer over the shorter. The
            // study list weighs the book's use of each (study_by_lemma).
            Stemmer::Pack(p) => match p.lemmas_of(word).as_slice() {
                [] => None,
                [one] => Some(*one),
                many => many
                    .iter()
                    .copied()
                    .find(|l| p.lemma(*l).is_some_and(|x| x.word == word))
                    .or_else(|| many.iter().copied().max_by_key(|l| (!p.is_little(*l), p.lemma(*l).map(|x| x.word.len()).unwrap_or(0)))),
            }
            .and_then(|l| p.lemma(l).map(|x| x.word.to_string()))
            .unwrap_or_else(|| word.to_string()),
        }
    }
}

// ------------------------------------------------------------------ words

/// The words of a text, as written: letters, with an apostrophe inside a
/// word kept ("don't", "l'Vivo" is "l'Vivo").
pub(super) fn words(text: &str) -> Vec<&str> {
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

/// Where an ASCII `needle` first occurs in `hay` at or after `from`, letter
/// case aside.
fn find_ascii_ci(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle)).map(|p| p + from)
}

/// The book's own text: Project Gutenberg's licence before and after it cut
/// away. The markers are ASCII, so they are found byte by byte, and a cut at
/// one falls on a character boundary.
pub(super) fn without_gutenberg(text: &str) -> &str {
    let mut text = text;
    for marker in ["*** START OF THE PROJECT GUTENBERG", "*** START OF THIS PROJECT GUTENBERG", "***START OF THE PROJECT GUTENBERG"] {
        if let Some(at) = find_ascii_ci(text.as_bytes(), marker.as_bytes(), 0) {
            if let Some(end) = find_ascii_ci(text.as_bytes(), b"***", at + 3) {
                text = &text[end + 3..];
            }
            break;
        }
    }
    for marker in ["*** END OF THE PROJECT GUTENBERG", "*** END OF THIS PROJECT GUTENBERG", "***END OF THE PROJECT GUTENBERG"] {
        if let Some(at) = find_ascii_ci(text.as_bytes(), marker.as_bytes(), 0) {
            text = &text[..at];
            break;
        }
    }
    text
}

// ------------------------------------------------------------- computing

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

/// The lines of a text that are in the pack's language. A line whose small
/// words are mostly unknown to the pack, or that has no word the pack knows,
/// is in another language (an English title page, credits or a licence in a
/// book in Swahili) and is left out. An unknown word with a capital is not
/// held against a line that has words of the language: names are unknown to
/// any pack.
fn in_language(text: &str, pack: &Pack) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split('\n') {
        let (mut known, mut unknown, mut names) = (0u32, 0u32, 0u32);
        for w in words(line) {
            if !pack.lemmas_of(&w.to_lowercase()).is_empty() {
                known += 1;
            } else if w.chars().next().is_some_and(char::is_uppercase) {
                names += 1;
            } else {
                unknown += 1;
            }
        }
        let foreign = (unknown >= 2 && known * 2 < known + unknown) || (known == 0 && unknown + names > 0);
        if foreign {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn compute(text: &str, language: &str, pack: Option<Arc<Pack>>) -> Option<Computed> {
    let stemmer = Stemmer::for_book(language, pack.clone())?;
    let text = without_gutenberg(text);
    let kept;
    let text = match pack.as_deref().filter(|_| language != "eo") {
        Some(p) => {
            kept = in_language(text, p);
            kept.as_str()
        }
        None => text,
    };
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
            let mut label = if language == "eo" && !name { esperanto::headword(stem, &f) } else { f[0].0.clone() };
            if name {
                let mut c = label.chars();
                label = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or(label);
            }
            Candidate { stem: (*stem).clone(), label, count: **n, name }
        })
        .collect();
    let study = if let Some(pack) = pack.filter(|_| language != "eo") {
        Some(study_by_lemma(&pack, &written, &stem_names, &is_name))
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
                        word: esperanto::headword(stem, &f),
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
fn study_by_lemma(pack: &Pack, written: &[(String, u32, u32)], stem_names: &[String], is_name: &dyn Fn(&str) -> bool) -> Vec<StudyWord> {
    let found: Vec<Vec<u32>> = written.iter().map(|(w, _, _)| pack.lemmas_of(w)).collect();
    let mut sure: HashMap<u32, u32> = HashMap::new();
    for ((_, n, _), lemmas) in written.iter().zip(&found) {
        if let [one] = lemmas.as_slice() {
            *sure.entry(*one).or_default() += n;
        }
    }
    // Key: the lemma's word, or the written word when the pack has none. A
    // name is kept under its written word and apart from any lemma spelt
    // the same, so that neither takes the other's count or flag.
    type Key = (String, bool);
    /// A study word's count and the forms it was written in.
    type Uses = (u32, HashMap<String, u32>);
    let mut by: HashMap<Key, Uses> = HashMap::new();
    let mut shown: HashMap<String, String> = HashMap::new();
    for ((word, n, stem), lemmas) in written.iter().zip(&found) {
        let name = is_name(&stem_names[*stem as usize]);
        let key = if name || lemmas.is_empty() {
            word.clone()
        } else {
            // A word that is a lemma itself stays itself ("a", "no", "casa")
            // unless the book uses another of its lemmas more in forms that
            // can only be that one ("era" as "ser", "hace" as "hacer"). A
            // little word is never a form of another little word, though:
            // "nos" is the pronoun however common "no" is.
            let own = lemmas.iter().copied().find(|l| {
                pack.lemma(*l).is_some_and(|x| x.word == word)
                    && lemmas
                        .iter()
                        .filter(|o| *o != l)
                        .all(|o| (pack.is_little(*l) && pack.is_little(*o)) || sure.get(o).copied().unwrap_or(0) < *n)
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
        let entry = by.entry((key, name)).or_insert_with(|| (0, HashMap::new()));
        entry.0 += n;
        *entry.1.entry(word.clone()).or_default() += n;
    }
    // A lone letter the pack does not know is an elision or an initial.
    let mut list: Vec<(Key, Uses)> = by.into_iter().filter(|((k, _), _)| k.chars().count() > 1 || pack.find(k).is_some()).collect();
    list.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    list.into_iter()
        .take(STUDY)
        .map(|((key, name), (count, forms))| {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn gutenbergs_licence_is_not_counted() {
        let text = "Header *** START OF THE PROJECT GUTENBERG EBOOK X *** the story *** END OF THE PROJECT GUTENBERG EBOOK X *** licence";
        assert_eq!(without_gutenberg(text).trim(), "the story");
        // Letters whose upper case takes another number of bytes (ı, ɐ) do
        // not stop the markers from being found; the case may vary.
        let text = "Kıssa ɐ *** Start of the Project Gutenberg EBook X *** the story *** END OF THIS PROJECT GUTENBERG EBOOK X *** ɐ ı";
        assert_eq!(without_gutenberg(text).trim(), "the story");
        assert_eq!(without_gutenberg("no markers here"), "no markers here");
    }

    fn spanish() -> Arc<Pack> {
        crate::langpack::fixture("es")
    }

    #[test]
    fn lines_in_another_language_are_left_out() {
        let es = spanish();
        let text = "Illustrations by René Leisink, adapted for this edition.\n\
                    Licensed under a Creative Commons licence; you may share it.\n\
                    Augusto Pérez y Eugenia Domingo\n\
                    El perro duerme en la casa y el gato mira por la ventana.\n";
        let kept = in_language(text, &es);
        assert!(!kept.contains("Illustrations") && !kept.contains("Licensed"), "{kept}");
        assert!(kept.contains("Augusto Pérez y Eugenia Domingo"), "{kept}");
        assert!(kept.contains("El perro duerme"), "{kept}");
        // A line without a word of the language (a label of the book's own)
        // goes; a heading of one word of the language stays.
        let kept = in_language("Illustrations\nThanks\nCasa\n", &es);
        assert_eq!(kept.trim(), "Casa");
    }

    #[test]
    fn a_little_word_that_is_a_lemma_of_its_own_is_not_a_form_of_another_little_word() {
        // "no" is everywhere; "nos" a few times, and is both the pronoun and
        // (as a plural) the noun "no".
        let mut text = String::new();
        for i in 0..200 {
            text.push_str("No, no quiero. No es así, no. ");
            if i % 20 == 0 {
                text.push_str("Nos vamos a casa y nos quedamos. ");
            }
        }
        let c = compute(&text, "es", Some(spanish())).unwrap();
        let study = c.study.unwrap();
        let nos = study.iter().find(|w| w.stem == "nos").unwrap_or_else(|| panic!("{study:?}"));
        assert!(nos.forms.contains(&"nos".to_string()));
        assert!(!study.iter().any(|w| w.stem == "no" && w.forms.contains(&"nos".to_string())), "{study:?}");
        // A content word's forms still go to the verb used ("es" to "ser").
        assert!(study.iter().any(|w| w.stem == "ser" && w.forms.contains(&"es".to_string())), "{study:?}");
    }

    #[test]
    fn a_name_spelt_like_a_lemma_is_kept_apart_from_it() {
        // "Ser" a surname, mid-sentence with a capital; "es" and "era" forms
        // of the verb "ser". The name must not become the verb's entry, nor
        // the verb a name.
        let mut text = String::new();
        for _ in 0..40 {
            text.push_str("El señor Ser llegó. Dijo que era tarde y que no es fácil. ");
        }
        let c = compute(&text, "es", Some(spanish())).unwrap();
        let study = c.study.unwrap();
        let rows: Vec<&StudyWord> = study.iter().filter(|w| w.stem == "ser").collect();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let name = rows.iter().find(|w| w.name).unwrap();
        let verb = rows.iter().find(|w| !w.name).unwrap();
        assert_eq!(name.word, "Ser");
        assert_eq!(name.count, 40);
        assert_eq!(name.forms, ["ser"]);
        assert_eq!(verb.word, "ser");
        assert!(verb.forms.contains(&"es".to_string()), "{verb:?}");
        assert!(!verb.forms.contains(&"ser".to_string()));
    }

    #[test]
    fn a_pack_stems_a_language_without_a_stemmer() {
        let es = spanish();
        assert!(Stemmer::for_book("xx", None).is_none());
        let stemmer = Stemmer::for_book("xx", Some(es.clone())).unwrap();
        assert!(matches!(stemmer, Stemmer::Pack(_)));
        assert_eq!(stemmer.stem("hizo"), "hacer");
        assert_eq!(stemmer.stem("nos"), "nos", "the lemma spelt like the form");
        assert_eq!(stemmer.stem("zzzzq"), "zzzzq", "unknown: as written");
        // Spanish keeps Snowball even with its pack.
        assert!(matches!(Stemmer::for_book("es", Some(es)).unwrap(), Stemmer::Snowball(_)));
    }
}
