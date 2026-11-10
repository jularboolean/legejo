//! Readability and style, measured: how long the sentences and the words
//! are, how varied the vocabulary is (MTLD, which unlike the share of
//! distinct words does not fall with the length of the text), how much of
//! the book is spoken, LIX and the language's own readability index, and
//! how the difficulty moves through the book.

use super::text::Prepared;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A word of more than this many letters is a long word (LIX).
const LONG_LETTERS: u32 = 6;
/// MTLD: a stretch of text is used up when its share of distinct words has
/// fallen to this.
const MTLD_FLOOR: f64 = 0.72;
/// Words before MTLD says anything.
const MTLD_WORDS: usize = 500;
/// The parts the book is cut into for the curve, and the words each needs.
const PARTS: usize = 20;
const PART_WORDS: usize = 1500;
const PARTS_MIN: usize = 5;
/// LIX points from the first part to the last before the book is said to
/// get harder or easier.
const DRIFT: f64 = 4.0;
/// The upper ends of the sentence lengths the histogram shows; longer
/// sentences are the last bar.
pub(super) const SENTENCE_STEPS: [u32; 6] = [5, 10, 15, 20, 30, 40];

/// The language's own readability index: higher is easier, about 0 to 100.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OwnIndex {
    /// "flesch", "fernandez-huerta", "amstad" or "gulpease".
    pub name: String,
    pub score: f64,
}

/// One part of the book on the curve.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Part {
    pub lix: f64,
    pub sentence: f64,
    pub spoken: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Style {
    /// Words and sentences measured: headings and labels are left out.
    pub words: u64,
    pub sentences: u64,
    /// Words per sentence: mean and median.
    pub sentence_mean: f64,
    pub sentence_median: f64,
    /// Sentences of 1–5, 6–10, 11–15, 16–20, 21–30, 31–40 and more words.
    pub sentence_spread: Vec<u64>,
    /// Letters per word, and the share of words of more than six letters.
    pub word_mean: f64,
    pub long_words: f64,
    /// MTLD; none for a text too short to tell.
    pub variation: Option<f64>,
    /// The share of the words that are spoken.
    pub spoken: f64,
    pub lix: f64,
    /// LIX as a level, 1 (very easy) to 5 (very hard), by Björnsson's limits.
    pub level: u32,
    pub own: Option<OwnIndex>,
    /// The book in equal parts, first to last; empty for a short book.
    pub curve: Vec<Part>,
    /// How much LIX changes from the first part to the last, and what that
    /// says: 1 harder towards the end, -1 easier, 0 even or up and down.
    pub drift: f64,
    pub trend: i32,
}

fn round(x: f64, decimals: i32) -> f64 {
    let k = 10f64.powi(decimals);
    (x * k).round() / k
}

fn lix(words: u64, sentences: u64, long: u64) -> f64 {
    if words == 0 || sentences == 0 {
        return 0.0;
    }
    words as f64 / sentences as f64 + 100.0 * long as f64 / words as f64
}

/// Björnsson's limits: below 25 children's books, to 35 fiction, to 45
/// newspapers, to 55 non-fiction, above that specialist prose.
fn lix_level(lix: f64) -> u32 {
    match lix {
        x if x < 25.0 => 1,
        x if x < 35.0 => 2,
        x if x < 45.0 => 3,
        x if x < 55.0 => 4,
        _ => 5,
    }
}

/// How many of the `ids` it takes, on average, before the share of distinct
/// words among them has fallen to the floor, read in one direction.
fn mtld_one_way(ids: impl Iterator<Item = u32>) -> f64 {
    let mut factors = 0.0;
    let mut seen: HashSet<u32> = HashSet::new();
    let mut tokens = 0usize;
    let mut total = 0usize;
    for id in ids {
        total += 1;
        tokens += 1;
        seen.insert(id);
        if (seen.len() as f64 / tokens as f64) <= MTLD_FLOOR {
            factors += 1.0;
            seen.clear();
            tokens = 0;
        }
    }
    if tokens > 0 {
        let ttr = seen.len() as f64 / tokens as f64;
        factors += (1.0 - ttr) / (1.0 - MTLD_FLOOR);
    }
    if factors == 0.0 {
        return 0.0;
    }
    total as f64 / factors
}

/// MTLD (McCarthy and Jarvis 2010): the mean of reading forwards and backwards.
fn mtld(ids: &[u32]) -> Option<f64> {
    if ids.len() < MTLD_WORDS {
        return None;
    }
    let (forward, backward) = (mtld_one_way(ids.iter().copied()), mtld_one_way(ids.iter().rev().copied()));
    (forward > 0.0 && backward > 0.0).then(|| (forward + backward) / 2.0)
}

fn is_vowel(c: char, vowels: &str) -> bool {
    vowels.contains(c)
}

/// Runs of vowels in a lower-case word: near enough the syllables of German
/// and, with the silent final e taken off, of English.
fn vowel_runs(word: &str, vowels: &str) -> u32 {
    let mut n = 0;
    let mut inside = false;
    for c in word.chars() {
        let v = is_vowel(c, vowels);
        if v && !inside {
            n += 1;
        }
        inside = v;
    }
    n
}

fn syllables_english(word: &str) -> u32 {
    let mut n = vowel_runs(word, "aeiouy");
    let consonant = |c: char| !is_vowel(c, "aeiouy");
    let ends_in_consonant = |w: &str| w.chars().next_back().is_some_and(consonant);
    // "made", "lives" and "liked" end in a silent e; "table", "houses" and
    // "wanted" do not.
    let silent = word.strip_suffix('e').is_some_and(|w| ends_in_consonant(w) && !w.ends_with('l'))
        || word.strip_suffix("es").is_some_and(|w| ends_in_consonant(w) && !w.ends_with(['s', 'x', 'z']) && !w.ends_with("ch") && !w.ends_with("sh"))
        || word.strip_suffix("ed").is_some_and(|w| ends_in_consonant(w) && !w.ends_with(['t', 'd']));
    if silent {
        n = n.saturating_sub(1);
    }
    n.max(1)
}

/// Spanish syllables: a new one at each vowel that does not join the vowel
/// before it. Two strong vowels (a, e, o) stand apart, and so does a weak
/// one with an accent (í, ú); a weak one without joins its neighbour.
fn syllables_spanish(word: &str) -> u32 {
    let strong = |c: char| "aeoáéóíú".contains(c);
    let weak = |c: char| "iuüy".contains(c);
    let mut n = 0;
    let mut before: Option<char> = None;
    let chars: Vec<char> = word.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        // "y" is a vowel only at the end of a word or alone ("y", "muy").
        let vowel = strong(c) || (weak(c) && (c != 'y' || i + 1 == chars.len()));
        if !vowel {
            before = None;
            continue;
        }
        match before {
            None => n += 1,
            Some(b) if strong(b) && strong(c) => n += 1,
            _ => {}
        }
        before = Some(c);
    }
    n.max(1)
}

/// The language's own index from words, sentences, syllables and letters.
fn own_index(language: &str, words: f64, sentences: f64, syllables: f64, letters: f64) -> Option<OwnIndex> {
    let per_sentence = words / sentences;
    let per_word = syllables / words;
    let (name, score) = match language {
        "en" => ("flesch", 206.835 - 1.015 * per_sentence - 84.6 * per_word),
        "es" => ("fernandez-huerta", 206.84 - 60.0 * per_word - 1.02 * per_sentence),
        "de" => ("amstad", 180.0 - per_sentence - 58.5 * per_word),
        "it" => ("gulpease", 89.0 + (300.0 * sentences - 10.0 * letters) / words),
        _ => return None,
    };
    Some(OwnIndex { name: name.to_string(), score: round(score, 1) })
}

pub(super) fn style(p: &Prepared, language: &str) -> Style {
    // Per distinct word: its letters and, where the language's index needs
    // them, its syllables.
    let letters: Vec<u32> = p.lower.iter().map(|w| w.chars().filter(|c| c.is_alphabetic()).count() as u32).collect();
    let syllables: Vec<u32> = match language {
        "en" => p.lower.iter().map(|w| syllables_english(w)).collect(),
        "es" => p.lower.iter().map(|w| syllables_spanish(w)).collect(),
        "de" => p.lower.iter().map(|w| vowel_runs(w, "aeiouyäöü").max(1)).collect(),
        _ => Vec::new(),
    };
    let words = p.tokens.len() as u64;
    let sentences = p.sentences.len() as u64;
    let (mut all_letters, mut all_syllables, mut long) = (0u64, 0u64, 0u64);
    for id in &p.ids {
        let l = letters[*id as usize];
        all_letters += l as u64;
        long += (l > LONG_LETTERS) as u64;
        all_syllables += syllables.get(*id as usize).copied().unwrap_or(0) as u64;
    }
    let spoken = p.spoken.iter().filter(|s| **s).count() as u64;

    let mut lengths: Vec<u32> = p.sentences.iter().map(|(a, b)| b - a).collect();
    let mut spread = vec![0u64; SENTENCE_STEPS.len() + 1];
    for len in &lengths {
        spread[SENTENCE_STEPS.iter().position(|step| len <= step).unwrap_or(SENTENCE_STEPS.len())] += 1;
    }
    lengths.sort_unstable();
    let median = match lengths.len() {
        0 => 0.0,
        n if n % 2 == 1 => lengths[n / 2] as f64,
        n => (lengths[n / 2 - 1] + lengths[n / 2]) as f64 / 2.0,
    };

    let parts = (p.tokens.len() / PART_WORDS).min(PARTS);
    let curve: Vec<Part> = if parts < PARTS_MIN {
        Vec::new()
    } else {
        let mut sentence = 0usize;
        p.parts(parts)
            .into_iter()
            .map(|(start, end)| {
                let mut in_part = 0u64;
                while sentence < p.sentences.len() && p.sentences[sentence].1 <= end {
                    in_part += 1;
                    sentence += 1;
                }
                let range = start as usize..end as usize;
                let n = range.len() as u64;
                let long = p.ids[range.clone()].iter().filter(|id| letters[**id as usize] > LONG_LETTERS).count() as u64;
                let spoken = p.spoken[range].iter().filter(|s| **s).count() as u64;
                Part {
                    lix: round(lix(n, in_part.max(1), long), 1),
                    sentence: round(n as f64 / in_part.max(1) as f64, 1),
                    spoken: round(spoken as f64 / n.max(1) as f64, 3),
                }
            })
            .collect()
    };
    let (drift, steady) = drift(&curve);

    let total_lix = lix(words, sentences, long);
    Style {
        words,
        sentences,
        sentence_mean: round(words as f64 / sentences.max(1) as f64, 1),
        sentence_median: median,
        sentence_spread: spread,
        word_mean: round(all_letters as f64 / words.max(1) as f64, 2),
        long_words: round(long as f64 / words.max(1) as f64, 3),
        variation: mtld(&p.ids).map(|v| round(v, 1)),
        spoken: round(spoken as f64 / words.max(1) as f64, 3),
        lix: round(total_lix, 1),
        level: lix_level(total_lix),
        own: if words > 0 && sentences > 0 { own_index(language, words as f64, sentences as f64, all_syllables as f64, all_letters as f64) } else { None },
        curve,
        drift: round(drift, 1),
        trend: if !steady || drift.abs() < DRIFT {
            0
        } else if drift > 0.0 {
            1
        } else {
            -1
        },
    }
}

/// How much LIX changes from the first part to the last along the straight
/// line that fits the curve best, and whether the curve follows that line
/// (a correlation of at least a half) rather than going up and down.
fn drift(curve: &[Part]) -> (f64, bool) {
    let n = curve.len() as f64;
    if curve.len() < PARTS_MIN {
        return (0.0, false);
    }
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = curve.iter().map(|p| p.lix).sum::<f64>() / n;
    let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
    for (i, part) in curve.iter().enumerate() {
        let (dx, dy) = (i as f64 - mean_x, part.lix - mean_y);
        xy += dx * dy;
        xx += dx * dx;
        yy += dy * dy;
    }
    if yy == 0.0 {
        return (0.0, false);
    }
    (xy / xx * (n - 1.0), (xy / (xx * yy).sqrt()).abs() >= 0.5)
}

#[cfg(test)]
mod tests {
    use super::super::text::prepare;
    use super::*;

    #[test]
    fn syllables_are_counted_as_the_indexes_need_them() {
        for (word, n) in [("the", 1), ("made", 1), ("table", 2), ("wanted", 2), ("liked", 1), ("lives", 1), ("houses", 2), ("beautiful", 3), ("rhythm", 1)] {
            assert_eq!(syllables_english(word), n, "{word}");
        }
        for (word, n) in [("casa", 2), ("que", 1), ("aire", 2), ("país", 2), ("poeta", 3), ("ciudad", 2), ("día", 2), ("muy", 1), ("yo", 1), ("guerra", 2), ("y", 1)] {
            assert_eq!(syllables_spanish(word), n, "{word}");
        }
    }

    #[test]
    fn sentences_words_and_lix_are_measured() {
        // Four sentences of 2, 4, 6 and 8 words; "elephants" and "wonderful" are long.
        let p = prepare("Dogs bark. The cat sleeps now. Two elephants walk to the river. It was a wonderful day in the park.");
        let s = style(&p, "en");
        assert_eq!((s.words, s.sentences), (20, 4));
        assert_eq!(s.sentence_mean, 5.0);
        assert_eq!(s.sentence_median, 5.0);
        assert_eq!(s.sentence_spread, [2, 2, 0, 0, 0, 0, 0]);
        assert_eq!(s.long_words, 0.1);
        assert_eq!(s.lix, 15.0);
        assert_eq!(s.level, 1);
        assert_eq!(s.own.as_ref().unwrap().name, "flesch");
        assert!(s.own.unwrap().score > 80.0);
        assert!(s.variation.is_none() && s.curve.is_empty(), "too short for either");
        assert_eq!(style(&p, "sv").own, None, "LIX is Swedish's own");
    }

    #[test]
    fn variation_does_not_depend_on_the_length() {
        // The same passage twice has half the share of distinct words and the same MTLD.
        let words = ["river", "stone", "light", "garden", "window", "letter", "morning", "silence", "bread", "road", "winter", "voice"];
        let mut once = String::new();
        for i in 0..300 {
            once.push_str(&format!("The {} and the {} met a {}. ", words[i % 12], words[(i * 5 + 1) % 12], words[(i * 7 + 3) % 12]));
        }
        let a = style(&prepare(&once), "en").variation.unwrap();
        let b = style(&prepare(&once.repeat(2)), "en").variation.unwrap();
        assert!((a - b).abs() < a * 0.05, "{a} {b}");
        // A text where two words in three are new is more varied than one of twelve words.
        let letters = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let varied: String = (0..3000).map(|i| if i % 3 == 0 { "the ".to_string() } else { format!("w{} ", letters[i % 10].repeat(1 + i / 10)) }).collect::<String>() + "end.";
        assert!(style(&prepare(&varied), "en").variation.unwrap() > a);
    }

    #[test]
    fn a_book_that_gets_harder_shows_it_on_its_curve() {
        let easy = "The dog ran to the old man. ".repeat(1200);
        let hard = "The extraordinary circumstances surrounding the mysterious disappearance troubled everybody considerably throughout the following afternoon and evening. ".repeat(600);
        let s = style(&prepare(&(easy.clone() + &hard)), "en");
        assert!(s.curve.len() >= PARTS_MIN, "{}", s.curve.len());
        assert!(s.curve[0].lix < s.curve[s.curve.len() - 1].lix);
        assert_eq!(s.trend, 1, "{}", s.drift);
        let s = style(&prepare(&(hard + &easy)), "en");
        assert_eq!(s.trend, -1);
        let s = style(&prepare(&easy.repeat(2)), "en");
        assert_eq!(s.trend, 0);
        // Half of this is spoken.
        let s = style(&prepare(&"“Come here now,” said the man. ".repeat(100)), "en");
        assert_eq!(s.spoken, 0.5);
    }
}
