//! Language packs: what it takes to give a book in a language a study list.
//! A pack maps every inflected form of the language to its lemma ("hizo" to
//! "hacer") and gives each lemma a short English meaning. Spanish, French
//! and German are built in; a pack for any other language can be put in the data directory under
//! `languages/<code>/`, in the same form (tools/langpack/README.md).
//!
//! A pack is four files:
//! - `pack.json`: its language, name, version, source and licence;
//! - `forms.fst`: an fst map from each form to a lemma's number, or, with
//!   bit 40 set, to a line of `ambiguous.tsv`;
//! - `ambiguous.tsv`: per line, the numbers of the lemmas a form can be;
//! - `lemmas.tsv`: per line (sorted, numbered from 0), a lemma, its parts of
//!   speech, its meaning and, when it is written otherwise (German nouns),
//!   the lemma as written.
//!
//! The fst is looked up where it lies (in the binary, or read once from the
//! disk); the lemmas are found by their line offsets, not parsed whole.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

const AMBIGUOUS: u64 = 1 << 40;

/// The packs in the binary: (code, pack.json, forms.fst, ambiguous.tsv, lemmas.tsv).
const BUILT_IN: &[(&str, &str, &[u8], &str, &str)] = &[
    (
        "es",
        include_str!("../assets/languages/es/pack.json"),
        include_bytes!("../assets/languages/es/forms.fst"),
        include_str!("../assets/languages/es/ambiguous.tsv"),
        include_str!("../assets/languages/es/lemmas.tsv"),
    ),
    (
        "fr",
        include_str!("../assets/languages/fr/pack.json"),
        include_bytes!("../assets/languages/fr/forms.fst"),
        include_str!("../assets/languages/fr/ambiguous.tsv"),
        include_str!("../assets/languages/fr/lemmas.tsv"),
    ),
    (
        "de",
        include_str!("../assets/languages/de/pack.json"),
        include_bytes!("../assets/languages/de/forms.fst"),
        include_str!("../assets/languages/de/ambiguous.tsv"),
        include_str!("../assets/languages/de/lemmas.tsv"),
    ),
];

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Meta {
    pub format: u32,
    pub language: String,
    pub name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    pub attribution: String,
}

pub struct Pack {
    pub meta: Meta,
    pub built_in: bool,
    forms: fst::Map<Cow<'static, [u8]>>,
    ambiguous: Vec<Vec<u32>>,
    lemmas: Cow<'static, str>,
    /// Where each line of `lemmas` starts.
    offsets: Vec<u32>,
}

/// One lemma: the word, its parts of speech ("prep,noun") and its meaning.
pub struct Lemma<'a> {
    /// The lemma's key: lower case, as forms are looked up.
    pub word: &'a str,
    /// As it is written ("Haus"); the key when nothing else is given.
    pub written: &'a str,
    pub pos: &'a str,
    pub english: &'a str,
}

/// Parts of speech of little words, which a form that is one of them is
/// taken as ("a" the preposition, not a form of another word).
const LITTLE: [&str; 10] = ["article", "prep", "conj", "pron", "det", "adv", "particle", "contraction", "num", "intj"];

impl Pack {
    fn new(meta: Meta, built_in: bool, forms: Cow<'static, [u8]>, ambiguous: &str, lemmas: Cow<'static, str>) -> anyhow::Result<Pack> {
        anyhow::ensure!(meta.format == 1, "pack format {} is not known", meta.format);
        let forms = fst::Map::new(forms)?;
        let ambiguous = ambiguous
            .lines()
            .map(|l| l.split(',').filter_map(|n| n.parse().ok()).collect())
            .collect();
        let mut offsets = Vec::new();
        let mut at = 0u32;
        for line in lemmas.split_inclusive('\n') {
            offsets.push(at);
            at += line.len() as u32;
        }
        Ok(Pack { meta, built_in, forms, ambiguous, lemmas, offsets })
    }

    pub fn lemma(&self, n: u32) -> Option<Lemma<'_>> {
        let start = *self.offsets.get(n as usize)? as usize;
        let end = self.offsets.get(n as usize + 1).map(|e| *e as usize).unwrap_or(self.lemmas.len());
        let mut parts = self.lemmas[start..end].trim_end_matches('\n').splitn(4, '\t');
        let word = parts.next()?;
        let pos = parts.next().unwrap_or("");
        let english = parts.next().unwrap_or("");
        let written = parts.next().filter(|w| !w.is_empty()).unwrap_or(word);
        Some(Lemma { word, written, pos, english })
    }

    /// A lemma by its word (the lines are sorted).
    pub fn find(&self, word: &str) -> Option<Lemma<'_>> {
        let n = self.offsets.binary_search_by(|&at| {
            let rest = &self.lemmas[at as usize..];
            rest.split('\t').next().unwrap_or("").cmp(word)
        });
        n.ok().and_then(|n| self.lemma(n as u32))
    }

    /// The lemmas a lower-case form can be.
    pub fn lemmas_of(&self, form: &str) -> Vec<u32> {
        match self.forms.get(form) {
            Some(v) if v & AMBIGUOUS != 0 => self.ambiguous.get((v & !AMBIGUOUS) as usize).cloned().unwrap_or_default(),
            Some(v) => vec![v as u32],
            None => Vec::new(),
        }
    }

    /// Whether a lemma is a little word (article, preposition and the like).
    pub fn is_little(&self, n: u32) -> bool {
        self.lemma(n).is_some_and(|l| l.pos.split(',').any(|p| LITTLE.contains(&p)))
    }
}

fn read_dir(dir: &Path) -> anyhow::Result<Pack> {
    let meta: Meta = serde_json::from_str(&std::fs::read_to_string(dir.join("pack.json"))?)?;
    let forms = std::fs::read(dir.join("forms.fst"))?;
    let ambiguous = std::fs::read_to_string(dir.join("ambiguous.tsv")).unwrap_or_default();
    let lemmas = std::fs::read_to_string(dir.join("lemmas.tsv"))?;
    Pack::new(meta, false, Cow::Owned(forms), &ambiguous, Cow::Owned(lemmas))
}

fn packs() -> &'static Mutex<HashMap<String, Option<Arc<Pack>>>> {
    static PACKS: OnceLock<Mutex<HashMap<String, Option<Arc<Pack>>>>> = OnceLock::new();
    PACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The pack for a language: one in the data directory first (so that an
/// operator can bring a newer one), else the built-in one. Loaded once.
pub fn get(data_dir: &Path, code: &str) -> Option<Arc<Pack>> {
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_lowercase()) || code.len() > 3 {
        return None;
    }
    if let Some(found) = packs().lock().unwrap().get(code) {
        return found.clone();
    }
    let dir = data_dir.join("languages").join(code);
    let found = if dir.join("pack.json").exists() {
        match read_dir(&dir) {
            Ok(p) if p.meta.language == code => Some(Arc::new(p)),
            Ok(p) => {
                tracing::warn!("language pack in {}: says it is for \"{}\"", dir.display(), p.meta.language);
                None
            }
            Err(e) => {
                tracing::warn!("language pack in {} cannot be read: {e:#}", dir.display());
                None
            }
        }
    } else {
        None
    };
    let found = found.or_else(|| {
        BUILT_IN.iter().find(|(c, ..)| *c == code).and_then(|(_, meta, forms, ambiguous, lemmas)| {
            let meta: Meta = serde_json::from_str(meta).ok()?;
            Pack::new(meta, true, Cow::Borrowed(forms), ambiguous, Cow::Borrowed(lemmas)).ok().map(Arc::new)
        })
    });
    packs().lock().unwrap().insert(code.to_string(), found.clone());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_is_built_in_and_finds_irregular_forms() {
        let none = Path::new("/nonexistent");
        let es = get(none, "es").unwrap();
        assert!(es.built_in);
        let word = |form: &str| es.lemmas_of(form).iter().map(|n| es.lemma(*n).unwrap().word.to_string()).collect::<Vec<_>>();
        assert_eq!(word("hizo"), ["hacer"]);
        assert_eq!(word("quiso"), ["querer"]);
        assert!(word("fue").contains(&"ser".to_string()) && word("fue").contains(&"ir".to_string()));
        assert!(es.find("casa").unwrap().english.contains("house"));
        assert!(es.find("a").unwrap().english.starts_with("to"));
        let a = es.lemmas_of("a");
        assert!(a.iter().any(|n| es.is_little(*n)));
        assert!(get(none, "sw").is_none());
    }

    #[test]
    fn french_and_german_are_built_in_and_german_nouns_keep_their_capital() {
        let none = Path::new("/nonexistent");
        let fr = get(none, "fr").unwrap();
        let word = |p: &Pack, form: &str| p.lemmas_of(form).iter().map(|n| p.lemma(*n).unwrap().word.to_string()).collect::<Vec<_>>();
        assert!(word(&fr, "était").contains(&"être".to_string()));
        assert_eq!(word(&fr, "allait"), ["aller"]);
        let de = get(none, "de").unwrap();
        assert_eq!(word(&de, "ging"), ["gehen"]);
        assert!(word(&de, "häuser").contains(&"haus".to_string()));
        assert_eq!(de.find("haus").unwrap().written, "Haus");
        assert_eq!(de.find("gehen").unwrap().written, "gehen");
    }
}
