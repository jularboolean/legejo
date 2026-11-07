//! Language packs: what it takes to give a book in a language a study list.
//! A pack maps every inflected form of the language to its lemma ("hizo" to
//! "hacer") and gives each lemma a short English meaning. Spanish, French
//! and German are built in; a pack for any other language, or a newer one
//! for these, can be put in the data directory under `languages/<code>/`
//! (tools/langpack/README.md).
//!
//! A pack is `pack.json` (its language, name, version, source and licence),
//! `lemmas.tsv` (per line: a lemma, its parts of speech, its meaning and,
//! when it is written otherwise (German nouns), the lemma as written; the
//! lines sorted by the lemma) and its forms, in one of two shapes:
//! - `forms.tsv`: per line a form and the lemmas it can be, in any order;
//!   the index below is built from it when the pack is loaded;
//! - `forms.fst` and `ambiguous.tsv`: the index pre-built, as the packs in
//!   the binary carry it: an fst map from each form to a lemma's line
//!   number, or, with bit 40 set, to a line of `ambiguous.tsv` listing the
//!   numbers of several.
//!
//! The fst is looked up where it lies (in the binary, or in memory); the
//! lemmas are found by their line offsets, not parsed whole.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

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
    /// The other names books give the language ("spa", "spanish"), which
    /// the catalog brings to its code.
    #[serde(default)]
    pub also: Vec<String>,
    /// Whether the pack is the one in the binary: set when it is loaded,
    /// never read from pack.json.
    #[serde(default, skip_deserializing)]
    pub built_in: bool,
}

pub struct Pack {
    pub meta: Meta,
    /// language:version:hash, which tells one pack's contents from another's.
    id: String,
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

/// The lines of ambiguous.tsv: per line, the numbers of several lemmas.
fn parse_ambiguous(text: &str) -> Vec<Vec<u32>> {
    text.lines().map(|l| l.split(',').filter_map(|n| n.parse().ok()).collect()).collect()
}

impl Pack {
    /// A pack from its parts; `ambiguous` is None when there is no
    /// ambiguous.tsv. Every value of the fst is checked against the lemmas
    /// and the ambiguous lines, so that a pack that does not fit together
    /// is refused here rather than looked up wrongly.
    fn new(mut meta: Meta, built_in: bool, forms: Cow<'static, [u8]>, ambiguous: Option<Vec<Vec<u32>>>, lemmas: Cow<'static, str>) -> anyhow::Result<Pack> {
        anyhow::ensure!(meta.format == 1, "pack format {} is not known", meta.format);
        meta.built_in = built_in;
        let mut hash = Sha256::new();
        hash.update(lemmas.as_bytes());
        hash.update(&forms);
        let hash = hash.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
        let id = format!("{}:{}:{}", meta.language, meta.version, &hash[..12]);
        let forms = fst::Map::new(forms)?;
        let mut offsets = Vec::new();
        let mut at = 0u32;
        for line in lemmas.split_inclusive('\n') {
            offsets.push(at);
            at += line.len() as u32;
        }
        let count = offsets.len() as u64;
        let ambiguous = ambiguous.unwrap_or_default();
        for (i, line) in ambiguous.iter().enumerate() {
            anyhow::ensure!(line.iter().all(|n| (*n as u64) < count), "ambiguous.tsv line {}: a lemma number beyond the {count} lemmas", i + 1);
        }
        use fst::Streamer;
        let mut stream = forms.stream();
        while let Some((form, value)) = stream.next() {
            let form = String::from_utf8_lossy(form);
            if value & AMBIGUOUS != 0 {
                anyhow::ensure!(!ambiguous.is_empty(), "\"{form}\" can be several lemmas but there is no ambiguous.tsv");
                anyhow::ensure!(((value & !AMBIGUOUS) as usize) < ambiguous.len(), "\"{form}\" points beyond ambiguous.tsv");
            } else {
                anyhow::ensure!(value < count, "\"{form}\" points at lemma {value}, beyond the {count} lemmas");
            }
        }
        Ok(Pack { meta, id, forms, ambiguous, lemmas, offsets })
    }

    /// language:version:hash of the pack's contents: a book counted with
    /// another pack is counted again.
    pub fn id(&self) -> &str {
        &self.id
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

    /// The keys of the lemmas whose English holds `q` (lower case), at most
    /// `cap` of them in the order of the lines: a pass over the lemmas, a
    /// few milliseconds.
    pub fn keys_with_gloss(&self, q: &str, cap: usize) -> Vec<&str> {
        let mut out = Vec::new();
        for line in self.lemmas.lines() {
            if out.len() >= cap {
                break;
            }
            let mut parts = line.splitn(4, '\t');
            let (Some(word), Some(_), Some(english)) = (parts.next(), parts.next(), parts.next()) else { continue };
            if english.to_lowercase().contains(q) {
                out.push(word);
            }
        }
        out
    }
}

/// Lines of forms.tsv that cannot be used are each reported, up to this many.
const FORM_WARNINGS: usize = 20;

/// The index of a pack given as text: the lemma lines sorted by key (the
/// line numbers come from the sorted text), the fst over the forms, and the
/// lines of several lemmas. A form pointing at a key the lemmas do not have
/// is reported and left out.
fn build_index(lemmas: &str, forms: &str) -> anyhow::Result<(String, Vec<u8>, Vec<Vec<u32>>)> {
    fn key_of(line: &str) -> &str {
        line.split('\t').next().unwrap_or("")
    }
    let mut lines: Vec<&str> = lemmas.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.sort_by(|a, b| key_of(a).cmp(key_of(b)));
    let keys: Vec<&str> = lines.iter().map(|l| key_of(l)).collect();
    let mut text = lines.join("\n");
    text.push('\n');
    let mut by_form: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    let mut warnings = 0usize;
    let mut warn = |what: String| {
        warnings += 1;
        if warnings <= FORM_WARNINGS {
            tracing::warn!("language pack: {what}");
        }
    };
    for (n, line) in forms.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((form, lemmas)) = line.split_once('\t') else {
            warn(format!("forms.tsv line {}: no tab between the form and its lemmas", n + 1));
            continue;
        };
        let entry = by_form.entry(form).or_default();
        for key in lemmas.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            match keys.binary_search(&key) {
                Ok(l) if !entry.contains(&(l as u32)) => entry.push(l as u32),
                Ok(_) => {}
                Err(_) => warn(format!("forms.tsv line {}: \"{form}\" points at \"{key}\", which lemmas.tsv does not have", n + 1)),
            }
        }
    }
    if warnings > FORM_WARNINGS {
        tracing::warn!("language pack: {} more lines of forms.tsv could not be used", warnings - FORM_WARNINGS);
    }
    let mut builder = fst::MapBuilder::memory();
    let mut ambiguous: Vec<Vec<u32>> = Vec::new();
    for (form, mut lemmas) in by_form {
        let value = match lemmas.len() {
            0 => continue,
            1 => lemmas[0] as u64,
            _ => {
                lemmas.sort_unstable();
                ambiguous.push(lemmas);
                AMBIGUOUS | (ambiguous.len() as u64 - 1)
            }
        };
        builder.insert(form, value)?;
    }
    Ok((text, builder.into_inner()?, ambiguous))
}

/// A pack from a directory: its index built from forms.tsv when that is
/// there, else read from forms.fst and ambiguous.tsv.
fn read_dir(dir: &Path) -> anyhow::Result<Pack> {
    let meta: Meta = serde_json::from_str(&std::fs::read_to_string(dir.join("pack.json")).context("pack.json")?).context("pack.json")?;
    let lemmas = std::fs::read_to_string(dir.join("lemmas.tsv")).context("lemmas.tsv")?;
    let text = dir.join("forms.tsv");
    if text.is_file() {
        let forms = std::fs::read_to_string(&text).context("forms.tsv")?;
        let (lemmas, fst, ambiguous) = build_index(&lemmas, &forms)?;
        return Pack::new(meta, false, Cow::Owned(fst), Some(ambiguous), Cow::Owned(lemmas));
    }
    let forms = std::fs::read(dir.join("forms.fst")).context("forms.tsv or forms.fst")?;
    let ambiguous = match std::fs::read_to_string(dir.join("ambiguous.tsv")) {
        Ok(text) => Some(parse_ambiguous(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(anyhow::Error::from(e).context("ambiguous.tsv")),
    };
    Pack::new(meta, false, Cow::Owned(forms), ambiguous, Cow::Owned(lemmas))
}

/// The pack for a language: one in the data directory first (so that an
/// operator can bring a newer one), else the built-in one.
fn load(data_dir: &Path, code: &str) -> Option<Arc<Pack>> {
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
    found.or_else(|| {
        BUILT_IN.iter().find(|(c, ..)| *c == code).and_then(|(_, meta, forms, ambiguous, lemmas)| {
            let meta: Meta = serde_json::from_str(meta).ok()?;
            match Pack::new(meta, true, Cow::Borrowed(forms), Some(parse_ambiguous(ambiguous)), Cow::Borrowed(lemmas)) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    tracing::error!("built-in language pack {code} cannot be read: {e:#}");
                    None
                }
            }
        })
    })
}

/// The packs an instance has: those in the data directory, else the ones in
/// the binary; each loaded once, when first asked for.
pub struct Packs {
    data_dir: PathBuf,
    cache: RwLock<HashMap<String, Option<Arc<Pack>>>>,
}

impl Packs {
    pub fn new(data_dir: PathBuf) -> Packs {
        Packs { data_dir, cache: RwLock::new(HashMap::new()) }
    }

    /// The pack for a language, or None when there is none (remembered too).
    pub fn get(&self, code: &str) -> Option<Arc<Pack>> {
        if !crate::language::is_code(code) {
            return None;
        }
        if let Some(found) = self.cache.read().unwrap().get(code) {
            return found.clone();
        }
        self.reload(code)
    }

    /// Load the pack for a language (again), from the disk or the binary.
    pub fn reload(&self, code: &str) -> Option<Arc<Pack>> {
        if !crate::language::is_code(code) {
            return None;
        }
        let found = load(&self.data_dir, code);
        self.cache.write().unwrap().insert(code.to_string(), found.clone());
        found
    }

    /// The packs there are, by language: a pack on the disk before the
    /// built-in one for the same language.
    pub fn installed(&self) -> Vec<Meta> {
        let mut codes: Vec<String> = BUILT_IN.iter().map(|(c, ..)| c.to_string()).collect();
        if let Ok(dir) = std::fs::read_dir(self.data_dir.join("languages")) {
            for entry in dir.flatten() {
                let Some(code) = entry.file_name().to_str().map(str::to_string) else { continue };
                if crate::language::is_code(&code) && entry.path().join("pack.json").is_file() && !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }
        codes.sort();
        codes.iter().filter_map(|c| self.get(c)).map(|p| p.meta.clone()).collect()
    }

    /// The names each installed pack's language goes by, with its code: the
    /// code itself and what `also` in pack.json says, in lower case.
    pub fn aliases(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for meta in self.installed() {
            out.push((meta.language.clone(), meta.language.clone()));
            for name in meta.also {
                out.push((name.to_lowercase(), meta.language.clone()));
            }
        }
        out
    }
}

/// A small pack for the made-up language "xx", as the builder writes one
/// without `--index`; the lemma lines in no order. "flib" is a noun and a
/// form of the verb "blorp".
#[cfg(test)]
pub(crate) fn write_xx(dir: &Path, gloss_for_blorp: &str, version: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("pack.json"),
        format!(
            r#"{{"format": 1, "language": "xx", "name": "Xxish", "version": "{version}", "source": "a test", "license": "CC0", "attribution": "the tests", "also": ["xxish", "xxx"]}}"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("lemmas.tsv"), format!("zam\tnoun\tcat\nblorp\tverb\t{gloss_for_blorp}\nko\tprep\tto\nflib\tnoun\tbox\n")).unwrap();
    std::fs::write(dir.join("forms.tsv"), "zams\tzam\nblorped\tblorp\nflib\tflib,blorp\nblorping\tblorp\nzam\tzam\nblorp\tblorp\nko\tko\nflib\tblorp\nglorf\tnothing\n").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built_in() -> Packs {
        Packs::new(PathBuf::from("/nonexistent"))
    }

    #[test]
    fn spanish_is_built_in_and_finds_irregular_forms() {
        let packs = built_in();
        let es = packs.get("es").unwrap();
        assert!(es.meta.built_in);
        assert!(es.id().starts_with("es:"), "{}", es.id());
        let word = |form: &str| es.lemmas_of(form).iter().map(|n| es.lemma(*n).unwrap().word.to_string()).collect::<Vec<_>>();
        assert_eq!(word("hizo"), ["hacer"]);
        assert_eq!(word("quiso"), ["querer"]);
        assert!(word("fue").contains(&"ser".to_string()) && word("fue").contains(&"ir".to_string()));
        assert!(es.find("casa").unwrap().english.contains("house"));
        assert!(es.find("a").unwrap().english.starts_with("to"));
        let a = es.lemmas_of("a");
        assert!(a.iter().any(|n| es.is_little(*n)));
        assert!(packs.get("sw").is_none());
        assert!(packs.get("Es").is_none() && packs.get("e").is_none() && packs.get("espa").is_none());
        let codes: Vec<String> = packs.installed().into_iter().map(|m| m.language).collect();
        assert_eq!(codes, ["de", "es", "fr"]);
        assert!(packs.aliases().contains(&("spanish".to_string(), "es".to_string())), "{:?}", packs.aliases());
    }

    #[test]
    fn french_and_german_are_built_in_and_german_nouns_keep_their_capital() {
        let packs = built_in();
        let fr = packs.get("fr").unwrap();
        let word = |p: &Pack, form: &str| p.lemmas_of(form).iter().map(|n| p.lemma(*n).unwrap().word.to_string()).collect::<Vec<_>>();
        assert!(word(&fr, "était").contains(&"être".to_string()));
        assert_eq!(word(&fr, "allait"), ["aller"]);
        let de = packs.get("de").unwrap();
        assert_eq!(word(&de, "ging"), ["gehen"]);
        assert!(word(&de, "häuser").contains(&"haus".to_string()));
        assert_eq!(de.find("haus").unwrap().written, "Haus");
        assert_eq!(de.find("gehen").unwrap().written, "gehen");
    }

    #[test]
    fn a_text_only_pack_is_built_at_load_and_wins_over_the_built_in() {
        let dir = std::env::temp_dir().join(format!("legejo-langpack-{}", crate::books::new_uuid()));
        write_xx(&dir.join("languages/xx"), "to run", "1");
        let packs = Packs::new(dir.clone());
        let xx = packs.get("xx").unwrap();
        assert!(!xx.meta.built_in);
        assert_eq!(xx.meta.also, ["xxish", "xxx"]);
        let words = |p: &Pack, form: &str| {
            let mut w: Vec<String> = p.lemmas_of(form).iter().map(|n| p.lemma(*n).unwrap().word.to_string()).collect();
            w.sort();
            w
        };
        assert_eq!(words(&xx, "blorped"), ["blorp"]);
        assert_eq!(words(&xx, "blorping"), ["blorp"]);
        assert_eq!(words(&xx, "zams"), ["zam"]);
        assert_eq!(words(&xx, "flib"), ["blorp", "flib"], "ambiguous, each lemma once");
        assert!(words(&xx, "glorf").is_empty(), "a form of a lemma that is not there is left out");
        assert_eq!(xx.find("blorp").unwrap().english, "to run");
        assert_eq!(xx.find("flib").unwrap().pos, "noun");
        assert!(xx.find("nothing").is_none());
        let ko = xx.lemmas_of("ko");
        assert!(ko.len() == 1 && xx.is_little(ko[0]));
        assert!(!xx.is_little(xx.lemmas_of("zam")[0]));
        let id = xx.id().to_string();
        assert!(id.starts_with("xx:1:"), "{id}");
        assert_eq!(packs.get("xx").unwrap().id(), id, "loaded once");
        assert!(packs.installed().iter().any(|m| m.language == "xx"));
        assert!(packs.aliases().contains(&("xxish".to_string(), "xx".to_string())));

        // Another gloss: another pack, by its id.
        write_xx(&dir.join("languages/xx"), "to sprint", "1");
        assert_eq!(packs.get("xx").unwrap().id(), id, "the first pack until reloaded");
        let again = packs.reload("xx").unwrap();
        assert_ne!(again.id(), id);
        assert_eq!(again.find("blorp").unwrap().english, "to sprint");

        // A disk pack for a built-in language wins over the one in the binary.
        write_xx(&dir.join("languages/es"), "to run", "1");
        std::fs::write(dir.join("languages/es/pack.json"), r#"{"format": 1, "language": "es", "name": "Mine", "version": "9", "source": "", "license": "", "attribution": ""}"#).unwrap();
        assert!(packs.get("es").unwrap().meta.built_in, "loaded before the disk pack came: until reloaded");
        let es = packs.reload("es").unwrap();
        assert_eq!(es.meta.name, "Mine");
        assert!(!es.meta.built_in);
        assert!(es.lemmas_of("hizo").is_empty());

        // A pre-built index whose fst has forms of several lemmas needs ambiguous.tsv.
        let broken = dir.join("languages/yy");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("pack.json"), r#"{"format": 1, "language": "yy", "name": "Yy", "version": "1", "source": "", "license": "", "attribution": ""}"#).unwrap();
        std::fs::write(broken.join("lemmas.tsv"), "a\tnoun\tx\nb\tnoun\ty\n").unwrap();
        let mut fst = fst::MapBuilder::memory();
        fst.insert("a", 0).unwrap();
        fst.insert("ab", AMBIGUOUS).unwrap();
        std::fs::write(broken.join("forms.fst"), fst.into_inner().unwrap()).unwrap();
        let err = read_dir(&broken).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("ambiguous.tsv"), "{err}");
        assert!(packs.get("yy").is_none());
        // And a value beyond the lemmas is refused too.
        std::fs::write(broken.join("ambiguous.tsv"), "0,1\n").unwrap();
        assert!(read_dir(&broken).is_ok());
        std::fs::write(broken.join("ambiguous.tsv"), "0,7\n").unwrap();
        assert!(read_dir(&broken).err().map(|e| e.to_string()).unwrap_or_default().contains("beyond"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
