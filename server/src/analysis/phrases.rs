//! Phrases and people. The phrases are the runs of two or three words that
//! the book writes together far more often than their words' own frequency
//! would give (pointwise mutual information): "sin embargo", "a pesar de".
//! The people and places are its names, how often each is named in each part
//! of the book, and which of them are named near each other.

use super::text::Prepared;
use crate::vocabulary::Stemmer;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Phrases shown.
const PHRASES: usize = 40;
/// A pair is a phrase when it is at least this many times (2^4) as common
/// as chance would have it; a third word must add as much to a pair.
const PAIR_PMI: f64 = 6.0;
const TRIPLE_PMI: f64 = 3.5;
/// A pair less unlikely than that is still a phrase when one of its words
/// is hardly written without the other ("de repente": "de" is everywhere,
/// "repente" only here).
const BOUND_PMI: f64 = 4.0;
const BOUND: f64 = 0.6;
/// A lone letter is part of a phrase only as a word of the language ("a",
/// "y", "I"): one at least this common. Others are abbreviations and labels.
const LETTER_SHARE: f64 = 0.001;
/// A pair inside a phrase of three is not shown beside it unless it occurs
/// this much more often on its own.
const INSIDE: f64 = 1.5;

/// Names shown, and how often a name must occur.
const NAMES: usize = 24;
const NAME_MIN: u32 = 3;
/// Names within the same run of this many words are named together.
const NEAR: usize = 100;
/// Times two names must be named together to be linked, and links shown.
const LINK_MIN: u32 = 2;
const LINKS: usize = 80;
/// The parts of the book a name's presence is given for.
const PARTS: usize = 20;
const PART_WORDS: usize = 500;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Phrase {
    pub text: String,
    pub count: u32,
    /// How many times as common as chance, as a power of two.
    pub pmi: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Name {
    pub name: String,
    pub count: u32,
    /// How often it is named in each part of the book, first to last.
    pub presence: Vec<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Link {
    /// Two names by their place in `names`, and how often they are named together.
    pub a: usize,
    pub b: usize,
    pub count: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct People {
    pub names: Vec<Name>,
    pub links: Vec<Link>,
}

/// The words that are names: written with a capital nearly every time they
/// stand inside a sentence. German writes every noun so, and has none here.
pub(super) fn name_words(p: &Prepared, language: &str) -> HashSet<u32> {
    if language == "de" {
        return HashSet::new();
    }
    let mut inside = vec![0u32; p.lower.len()];
    let mut capitals = vec![0u32; p.lower.len()];
    for (i, token) in p.tokens.iter().enumerate() {
        if p.first[i] {
            continue;
        }
        let id = p.ids[i] as usize;
        inside[id] += 1;
        capitals[id] += token.chars().next().is_some_and(char::is_uppercase) as u32;
    }
    (0..p.lower.len() as u32)
        .filter(|id| {
            let (n, c) = (inside[*id as usize], capitals[*id as usize]);
            n >= 2 && c as f64 > n as f64 * 0.8 && p.word(*id).chars().count() > 1
        })
        .collect()
}

fn log2_ratio(together: f64, apart: f64) -> f64 {
    (together / apart).log2()
}

pub(super) fn phrases(p: &Prepared, names: &HashSet<u32>) -> Vec<Phrase> {
    let total = p.tokens.len() as f64;
    if total < 1000.0 {
        return Vec::new();
    }
    let min = 4 + (p.tokens.len() / 30_000) as u32;
    let counts = p.counts();
    let n = |id: u32| counts[id as usize] as f64;
    let mut pairs: HashMap<(u32, u32), u32> = HashMap::new();
    let mut triples: HashMap<(u32, u32, u32), u32> = HashMap::new();
    for (start, end) in &p.sentences {
        let ids = &p.ids[*start as usize..*end as usize];
        for w in ids.windows(2) {
            *pairs.entry((w[0], w[1])).or_default() += 1;
        }
        for w in ids.windows(3) {
            *triples.entry((w[0], w[1], w[2])).or_default() += 1;
        }
    }
    let named = |ids: &[u32]| ids.iter().any(|id| names.contains(id) || (p.word(*id).chars().count() == 1 && n(*id) < total * LETTER_SHARE));
    // (score, words, count, pmi)
    let mut found: Vec<(f64, Vec<u32>, u32, f64)> = Vec::new();
    let mut inside: HashMap<(u32, u32), u32> = HashMap::new();
    for (&(a, b, c), &count) in &triples {
        if count < min || named(&[a, b, c]) {
            continue;
        }
        let (ab, bc) = (pairs[&(a, b)] as f64, pairs[&(b, c)] as f64);
        // What the third word adds to the pair, from either end: the lesser.
        let pmi = log2_ratio(count as f64 * total, ab * n(c)).min(log2_ratio(count as f64 * total, n(a) * bc));
        if pmi < TRIPLE_PMI {
            continue;
        }
        for pair in [(a, b), (b, c)] {
            let most = inside.entry(pair).or_default();
            *most = (*most).max(count);
        }
        found.push((pmi * (count as f64).log2() * 1.2, vec![a, b, c], count, pmi));
    }
    for (&(a, b), &count) in &pairs {
        if count < min || named(&[a, b]) || a == b {
            continue;
        }
        if inside.get(&(a, b)).is_some_and(|most| (count as f64) < *most as f64 * INSIDE) {
            continue;
        }
        let pmi = log2_ratio(count as f64 * total, n(a) * n(b));
        let bound = count as f64 / n(a).min(n(b));
        if pmi < PAIR_PMI && (pmi < BOUND_PMI || bound < BOUND) {
            continue;
        }
        found.push((pmi * (count as f64).log2(), vec![a, b], count, pmi));
    }
    found.sort_by(|x, y| y.0.total_cmp(&x.0).then_with(|| x.1.cmp(&y.1)));
    found
        .into_iter()
        .take(PHRASES)
        .map(|(_, ids, count, pmi)| Phrase {
            text: ids.iter().map(|id| p.word(*id)).collect::<Vec<_>>().join(" "),
            count,
            pmi: (pmi * 10.0).round() / 10.0,
        })
        .collect()
}

pub(super) fn people(p: &Prepared, names: &HashSet<u32>, stemmer: Option<&Stemmer>, stop: &HashSet<String>) -> People {
    // A name in its several forms ("Petro", "Petron") is one name: by stem.
    let mut keys: HashMap<String, usize> = HashMap::new();
    let mut key_of: HashMap<u32, usize> = HashMap::new();
    // In the order the book first writes them, which settles ties.
    let mut ids: Vec<&u32> = names.iter().collect();
    ids.sort_unstable();
    for id in ids {
        // "Darcy’s" is Darcy.
        let word = p.word(*id);
        let word = word.strip_suffix("’s").or_else(|| word.strip_suffix("'s")).unwrap_or(word);
        let stem = stemmer.map(|s| s.stem(word)).unwrap_or_else(|| word.to_string());
        if stop.contains(word) || stop.contains(&stem) {
            continue;
        }
        let next = keys.len();
        key_of.insert(*id, *keys.entry(stem).or_insert(next));
    }
    if keys.is_empty() {
        return People::default();
    }
    let parts = (p.tokens.len() / PART_WORDS).clamp(1, PARTS);
    let mut count = vec![0u32; keys.len()];
    let mut presence = vec![vec![0u32; parts]; keys.len()];
    let mut written: Vec<HashMap<&str, u32>> = vec![HashMap::new(); keys.len()];
    for (part, (start, end)) in p.parts(parts).into_iter().enumerate() {
        for i in start as usize..end as usize {
            let Some(&key) = key_of.get(&p.ids[i]) else { continue };
            count[key] += 1;
            presence[key][part] += 1;
            // The form shown is the commonest as written inside a sentence.
            if !p.first[i] {
                *written[key].entry(p.tokens[i]).or_default() += 1;
            }
        }
    }
    let mut order: Vec<usize> = (0..keys.len()).filter(|k| count[*k] >= NAME_MIN).collect();
    order.sort_by(|a, b| count[*b].cmp(&count[*a]).then_with(|| a.cmp(b)));
    order.truncate(NAMES);
    let place: HashMap<usize, usize> = order.iter().enumerate().map(|(place, key)| (*key, place)).collect();

    let mut together: HashMap<(usize, usize), u32> = HashMap::new();
    for run in p.ids.chunks(NEAR) {
        let mut here: Vec<usize> = run.iter().filter_map(|id| key_of.get(id)).filter_map(|key| place.get(key)).copied().collect();
        here.sort_unstable();
        here.dedup();
        for (i, a) in here.iter().enumerate() {
            for b in &here[i + 1..] {
                *together.entry((*a, *b)).or_default() += 1;
            }
        }
    }
    let mut links: Vec<Link> = together.into_iter().filter(|(_, n)| *n >= LINK_MIN).map(|((a, b), count)| Link { a, b, count }).collect();
    links.sort_by(|x, y| y.count.cmp(&x.count).then_with(|| (x.a, x.b).cmp(&(y.a, y.b))));
    links.truncate(LINKS);

    let names = order
        .into_iter()
        .map(|key| {
            let mut forms: Vec<(&str, u32)> = written[key].iter().map(|(w, n)| (*w, *n)).collect();
            forms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.len().cmp(&b.0.len())).then_with(|| a.0.cmp(b.0)));
            Name { name: forms.first().map(|(w, _)| w.to_string()).unwrap_or_default(), count: count[key], presence: std::mem::take(&mut presence[key]) }
        })
        .filter(|n| !n.name.is_empty())
        .collect();
    People { names, links }
}

#[cfg(test)]
mod tests {
    use super::super::text::prepare;
    use super::*;

    /// A text of filler sentences whose words fall in no fixed order, with
    /// `extra` put in after every tenth.
    fn filler(sentences: usize, extra: &dyn Fn(usize) -> String) -> String {
        let words = ["casa", "perro", "noche", "calle", "mesa", "libro", "agua", "puerta", "mano", "tiempo", "ojos", "vida", "luz"];
        let mut text = String::new();
        for i in 0..sentences {
            let w = |k: usize| words[(i * k + k * k + i / 7) % words.len()];
            text.push_str(&format!("La {} en la {} no era como la {} ni el {}. ", w(1), w(3), w(5), w(7)));
            if i % 10 == 0 {
                text.push_str(&extra(i));
            }
        }
        text
    }

    #[test]
    fn fixed_phrases_are_found_and_chance_neighbours_are_not() {
        let words = ["mujer", "tarde", "carta", "ciudad", "lluvia", "madre", "gente"];
        let text = filler(600, &|i| {
            let (a, b) = (words[i / 10 % 7], words[i / 10 % 5]);
            if i % 20 == 0 {
                format!("Sin embargo la {a} no era suya. ")
            } else {
                format!("La {a} llegó a pesar de la {b}. ")
            }
        });
        let p = prepare(&text);
        let found = phrases(&p, &name_words(&p, "es"));
        let texts: Vec<&str> = found.iter().map(|f| f.text.as_str()).collect();
        assert!(texts.contains(&"sin embargo"), "{texts:?}");
        assert!(texts.contains(&"a pesar de"), "{texts:?}");
        // The pairs inside the phrase of three are not listed beside it, and
        // "en la", common as it is, is no phrase.
        assert!(!texts.contains(&"a pesar") && !texts.contains(&"pesar de"), "{texts:?}");
        assert!(!texts.contains(&"en la") && !texts.contains(&"sin embargo la"), "{texts:?}");
        let sin = found.iter().find(|f| f.text == "sin embargo").unwrap();
        assert_eq!(sin.count, 30);
        assert!(sin.pmi > BOUND_PMI);
    }

    #[test]
    fn names_are_counted_placed_and_linked() {
        // Augusto throughout, with Eugenia in the first half and Rosario in
        // the second; Eugenia and Rosario never near each other.
        let text = filler(1000, &|i| {
            let other = if i < 500 { "Eugenia" } else { "Rosario" };
            format!("Entonces habló Augusto con {other} en la calle. ")
        });
        let p = prepare(&text);
        let names = name_words(&p, "es");
        assert!(name_words(&p, "de").is_empty());
        let stop: HashSet<String> = ["entonces".to_string()].into();
        let people = people(&p, &names, Stemmer::for_language("es").as_ref(), &stop);
        let shown: Vec<&str> = people.names.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(shown, ["Augusto", "Eugenia", "Rosario"], "{shown:?}");
        assert_eq!(people.names[0].count, 100);
        let eugenia = &people.names[1].presence;
        assert_eq!(eugenia.len(), PARTS);
        assert!(eugenia[0] > 0 && eugenia[PARTS - 1] == 0, "{eugenia:?}");
        let linked: Vec<(usize, usize)> = people.links.iter().map(|l| (l.a, l.b)).collect();
        assert!(linked.contains(&(0, 1)) && linked.contains(&(0, 2)), "{linked:?}");
        assert!(!linked.contains(&(1, 2)), "{linked:?}");
    }
}
