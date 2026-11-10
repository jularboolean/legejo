//! Language packs: what it takes to give a book in a language a study list.
//! A pack maps every inflected form of the language to its lemma ("hizo" to
//! "hacer") and gives each lemma a short English meaning. Packs live in the
//! data directory under `languages/<code>/`: installed from the catalogue
//! (langcatalogue.rs), imported from a zip (langadmin.rs) or put there by
//! hand (tools/langpack/README.md). None is in the binary; Esperanto has
//! ESPDIC instead (espdic.rs).
//!
//! A pack is `pack.json` (its language, name, version, source and licence),
//! `lemmas.tsv` (per line: a lemma, its parts of speech, its meaning and,
//! when it is written otherwise (German nouns), the lemma as written; the
//! lines sorted by the lemma) and its forms, in one of two shapes:
//! - `forms.tsv`: per line a form and the lemmas it can be, in any order;
//!   the index below is built from it when the pack is loaded;
//! - `forms.fst` and `ambiguous.tsv`: the index pre-built (`langpack
//!   --index`): an fst map from each form to a lemma's line number, or, with
//!   bit 40 set, to a line of `ambiguous.tsv` listing the numbers of several.
//!
//! The fst is looked up in memory; the lemmas are found by their line
//! offsets, not parsed whole.
//!
//! A pack may have extras beside it, for the analysis of a book
//! (analysis/): `tags.tsv` (per line a form, its lemma and what the form
//! is: "hizo", "hacer", "third-person singular preterite indicative"; the
//! several things it can be separated by semicolons), `frequency.tsv` (the
//! lemmas in order of frequency, one per line) and `extras.json` (what
//! there is and where the frequency list comes from). They are read when
//! first asked for, not with the pack, and are not part of its id: a book's
//! words are not counted again when extras come.

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const AMBIGUOUS: u64 = 1 << 40;

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
}

/// extras.json.
#[derive(Deserialize, Default, Clone, Debug)]
pub struct ExtrasMeta {
    /// Where the frequency list comes from and its licence, to show with
    /// what is worked out from it.
    #[serde(default)]
    pub frequency: Option<String>,
}

/// A pack's extras, as read from its directory.
pub struct Extras {
    pub meta: ExtrasMeta,
    /// A hash of the files, which tells one set of extras from another.
    id: String,
    /// "lemma\tform" -> a place in `kinds`; None without tags.tsv.
    tags: Option<fst::Map<Vec<u8>>>,
    /// The distinct descriptions of tags.tsv ("plural", "first-person
    /// singular present;third-person singular present").
    kinds: Vec<String>,
    /// A lemma's line number -> its rank, the commonest 1.
    ranks: HashMap<u32, u32>,
}

impl Extras {
    /// What a form of a lemma is: each thing it can be, as its tags.
    pub fn tags_of(&self, lemma: &str, form: &str) -> Vec<Vec<&str>> {
        let Some(n) = self.tags.as_ref().and_then(|t| t.get(format!("{lemma}\t{form}"))) else { return Vec::new() };
        self.kinds[n as usize].split(';').map(|kind| kind.split(' ').collect()).collect()
    }

    /// The forms of a lemma that tags.tsv describes, each with what it can be.
    pub fn forms_of(&self, lemma: &str) -> Vec<(String, Vec<Vec<&str>>)> {
        use fst::{IntoStreamer, Streamer};
        let Some(tags) = &self.tags else { return Vec::new() };
        let prefix = format!("{lemma}\t");
        let mut stream = tags.range().ge(prefix.as_bytes()).into_stream();
        let mut out = Vec::new();
        while let Some((key, n)) = stream.next() {
            let Some(form) = std::str::from_utf8(key).ok().and_then(|k| k.strip_prefix(prefix.as_str())) else { break };
            out.push((form.to_string(), self.kinds[n as usize].split(';').map(|kind| kind.split(' ').collect()).collect()));
        }
        out
    }

    pub fn has_tags(&self) -> bool {
        self.tags.is_some()
    }

    pub fn has_frequency(&self) -> bool {
        !self.ranks.is_empty()
    }

    /// How many lemmas the frequency list ranks.
    pub fn ranked(&self) -> usize {
        self.ranks.len()
    }

    /// A lemma's rank by frequency (the commonest 1), by its line number.
    pub fn rank(&self, lemma: u32) -> Option<u32> {
        self.ranks.get(&lemma).copied()
    }
}

/// The index over tags.tsv, read a line at a time: the file of a large
/// language is tens of megabytes, and is never held whole. The builder
/// writes the lines in the order of the index (by lemma, then form), and
/// then each goes straight into it; a file in another order (edited by
/// hand) is read again, its keys sorted first.
fn read_tags(path: &Path, hash: &mut Sha256, kinds: &mut Vec<String>) -> anyhow::Result<fst::Map<Vec<u8>>> {
    use std::io::BufRead;
    /// The lines as (key, kind), each kind numbered as it is first seen.
    fn lines<'a>(
        path: &Path,
        kinds: &'a mut Vec<String>,
        kind_ids: &'a mut HashMap<String, u64>,
        mut hash: Option<&'a mut Sha256>,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<(String, u64)>> + 'a> {
        let reader = std::io::BufReader::new(std::fs::File::open(path)?);
        Ok(reader.lines().filter_map(move |line| {
            let line = match line {
                Ok(line) => line,
                Err(e) => return Some(Err(e.into())),
            };
            if let Some(hash) = hash.as_deref_mut() {
                hash.update(line.as_bytes());
                hash.update(b"\n");
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(form), Some(lemma), Some(kind)) = (parts.next(), parts.next(), parts.next()) else { return None };
            if form.is_empty() || lemma.is_empty() || kind.is_empty() {
                return None;
            }
            let id = match kind_ids.get(kind) {
                Some(id) => *id,
                None => {
                    let id = kinds.len() as u64;
                    kind_ids.insert(kind.to_string(), id);
                    kinds.push(kind.to_string());
                    id
                }
            };
            Some(Ok((format!("{lemma}\t{form}"), id)))
        }))
    }
    let mut kind_ids: HashMap<String, u64> = HashMap::new();
    let mut builder = fst::MapBuilder::memory();
    let mut last: Option<String> = None;
    let mut in_order = true;
    for entry in lines(path, kinds, &mut kind_ids, Some(hash))? {
        let (key, id) = entry?;
        match last.as_deref().map(|l| l.cmp(key.as_str())) {
            Some(std::cmp::Ordering::Equal) => continue,
            Some(std::cmp::Ordering::Greater) => {
                in_order = false;
                break;
            }
            _ => {}
        }
        builder.insert(&key, id)?;
        last = Some(key);
    }
    if in_order {
        return Ok(fst::Map::new(builder.into_inner()?)?);
    }
    // Not in order: every key at once, sorted. The kinds keep their numbers.
    let mut keys: Vec<(String, u64)> = lines(path, kinds, &mut kind_ids, None)?.collect::<anyhow::Result<_>>()?;
    keys.sort();
    keys.dedup_by(|a, b| a.0 == b.0);
    let mut builder = fst::MapBuilder::memory();
    for (key, id) in keys {
        builder.insert(key, id)?;
    }
    Ok(fst::Map::new(builder.into_inner()?)?)
}

/// The extras in a pack's directory; None when it has neither tags.tsv nor
/// frequency.tsv. A line that cannot be used is passed over.
fn read_extras(dir: &Path, pack: &Pack) -> anyhow::Result<Option<Extras>> {
    let read = |name: &str| match std::fs::read_to_string(dir.join(name)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::from(e).context(name.to_string())),
    };
    let tags_path = dir.join("tags.tsv");
    let frequency_text = read("frequency.tsv")?;
    if !tags_path.is_file() && frequency_text.is_none() {
        return Ok(None);
    }
    let meta: ExtrasMeta = match read("extras.json")? {
        Some(json) => serde_json::from_str(&json).context("extras.json")?,
        None => ExtrasMeta::default(),
    };
    let mut hash = Sha256::new();
    let mut kinds: Vec<String> = Vec::new();
    let tags = if tags_path.is_file() { Some(read_tags(&tags_path, &mut hash, &mut kinds).context("tags.tsv")?) } else { None };
    let mut ranks = HashMap::new();
    if let Some(text) = &frequency_text {
        hash.update(text.as_bytes());
        for word in text.lines().map(str::trim).filter(|w| !w.is_empty()) {
            if let Some(n) = pack.line_of(word) {
                let rank = ranks.len() as u32 + 1;
                ranks.entry(n).or_insert(rank);
            }
        }
    }
    let id = hash.finalize().iter().take(6).map(|b| format!("{b:02x}")).collect();
    Ok(Some(Extras { meta, id, tags, kinds, ranks }))
}

pub struct Pack {
    pub meta: Meta,
    /// language:version:hash, which tells one pack's contents from another's.
    id: String,
    forms: fst::Map<Vec<u8>>,
    ambiguous: Vec<Vec<u32>>,
    lemmas: String,
    /// Where each line of `lemmas` starts.
    offsets: Vec<u32>,
    /// Where the pack was read from, for its extras.
    dir: Option<PathBuf>,
    extras: std::sync::OnceLock<Option<Extras>>,
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
    fn new(meta: Meta, forms: Vec<u8>, ambiguous: Option<Vec<Vec<u32>>>, lemmas: String) -> anyhow::Result<Pack> {
        anyhow::ensure!(meta.format == 1, "pack format {} is not known", meta.format);
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
        Ok(Pack { meta, id, forms, ambiguous, lemmas, offsets, dir: None, extras: Default::default() })
    }

    /// The pack's extras, read from its directory the first time they are
    /// asked for; None when it has none or they cannot be read.
    pub fn extras(&self) -> Option<&Extras> {
        self.extras
            .get_or_init(|| {
                let dir = self.dir.as_deref()?;
                read_extras(dir, self).unwrap_or_else(|e| {
                    tracing::warn!("language pack in {}: its extras cannot be read: {e:#}", dir.display());
                    None
                })
            })
            .as_ref()
    }

    /// Whether the pack's directory holds extras (not read for this).
    pub fn has_extras(&self) -> bool {
        self.dir.as_deref().is_some_and(|d| d.join("tags.tsv").is_file() || d.join("frequency.tsv").is_file())
    }

    /// The pack's id with that of its extras: what a book's analysis was
    /// worked out with.
    pub fn id_with_extras(&self) -> String {
        match self.extras() {
            Some(extras) => format!("{}+{}", self.id, extras.id),
            None => self.id.clone(),
        }
    }

    /// language:version:hash of the pack's contents: a book counted with
    /// another pack is counted again.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// How many lemmas the pack has.
    pub fn lemma_count(&self) -> usize {
        self.offsets.len()
    }

    /// How many forms the pack can look up.
    pub fn form_count(&self) -> usize {
        self.forms.len()
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

    /// A lemma's line number by its word (the lines are sorted).
    pub fn line_of(&self, word: &str) -> Option<u32> {
        let n = self.offsets.binary_search_by(|&at| {
            let rest = &self.lemmas[at as usize..];
            rest.split('\t').next().unwrap_or("").cmp(word)
        });
        n.ok().map(|n| n as u32)
    }

    /// A lemma by its word.
    pub fn find(&self, word: &str) -> Option<Lemma<'_>> {
        self.line_of(word).and_then(|n| self.lemma(n))
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
        return Pack::new(meta, fst, Some(ambiguous), lemmas).map(|p| Pack { dir: Some(dir.to_path_buf()), ..p });
    }
    let forms = std::fs::read(dir.join("forms.fst")).context("forms.tsv or forms.fst")?;
    let ambiguous = match std::fs::read_to_string(dir.join("ambiguous.tsv")) {
        Ok(text) => Some(parse_ambiguous(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(anyhow::Error::from(e).context("ambiguous.tsv")),
    };
    Pack::new(meta, forms, ambiguous, lemmas).map(|p| Pack { dir: Some(dir.to_path_buf()), ..p })
}

/// A pack in a directory checked as `load` would read it for `code`, with
/// what an imported pack must have besides: lemmas, and forms that point at
/// them. The importer (langadmin.rs) calls it on the unpacked files before
/// they take the place of the pack there is.
pub fn check_dir(dir: &Path, code: &str) -> anyhow::Result<Pack> {
    let pack = read_dir(dir)?;
    anyhow::ensure!(pack.meta.language == code, "pack.json says the pack is for \"{}\", not \"{code}\"", pack.meta.language);
    anyhow::ensure!(pack.lemma_count() > 0, "lemmas.tsv has no lemmas");
    anyhow::ensure!(pack.form_count() > 0, "no line of forms.tsv points at a lemma of lemmas.tsv");
    // Extras that are there must be readable; a pack without any is whole.
    read_extras(dir, &pack)?;
    Ok(pack)
}

/// The pack for a language in the data directory, if there is one that loads.
fn load(data_dir: &Path, code: &str) -> Option<Arc<Pack>> {
    let dir = data_dir.join("languages").join(code);
    if !dir.join("pack.json").exists() {
        return None;
    }
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
}

/// The packs an instance has, in the data directory; each loaded once, when
/// first asked for.
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

    /// Load the pack for a language (again) from the disk.
    pub fn reload(&self, code: &str) -> Option<Arc<Pack>> {
        if !crate::language::is_code(code) {
            return None;
        }
        let found = load(&self.data_dir, code);
        self.cache.write().unwrap().insert(code.to_string(), found.clone());
        found
    }

    /// Where the packs on the disk are: `<data dir>/languages`, always on
    /// the local disk, also when the data directory is kept in a bucket
    /// (storage.rs does not carry them).
    pub fn dir(&self) -> PathBuf {
        self.data_dir.join("languages")
    }

    /// The packs there are, by language.
    pub fn installed(&self) -> Vec<Meta> {
        self.installed_packs().iter().map(|p| p.meta.clone()).collect()
    }

    /// As `installed`, the packs themselves.
    pub fn installed_packs(&self) -> Vec<Arc<Pack>> {
        let mut codes: Vec<String> = Vec::new();
        if let Ok(dir) = std::fs::read_dir(self.dir()) {
            for entry in dir.flatten() {
                let Some(code) = entry.file_name().to_str().map(str::to_string) else { continue };
                if crate::language::is_code(&code) && entry.path().join("pack.json").is_file() && !codes.contains(&code) {
                    codes.push(code);
                }
            }
        }
        codes.sort();
        codes.iter().filter_map(|c| self.get(c)).collect()
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

/// Extras for the made-up pack of `write_xx`: "flib" is the past of blorp
/// or a noun in the singular, and "ko" the commonest word.
#[cfg(test)]
pub(crate) fn write_xx_extras(dir: &Path) {
    std::fs::write(dir.join("tags.tsv"), "blorped\tblorp\tpast\nblorping\tblorp\tgerund\nflib\tblorp\tthird-person singular past;first-person singular past\nzams\tzam\tplural\nbroken line\n").unwrap();
    std::fs::write(dir.join("frequency.tsv"), "ko\nblorp\nnothing\nzam\n").unwrap();
    std::fs::write(dir.join("extras.json"), r#"{"format": 1, "language": "xx", "version": "1", "tags": true, "frequency": "A word list, CC0"}"#).unwrap();
}

/// Where the packs the tests use are: Spanish, French and German as
/// `langpack --index` writes them (server/tests/packs/<code>).
#[cfg(test)]
pub(crate) fn fixture_dir(code: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/packs").join(code)
}

/// A test pack, loaded once per test process.
#[cfg(test)]
pub(crate) fn fixture(code: &str) -> Arc<Pack> {
    static LOADED: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Arc<Pack>>>> = std::sync::OnceLock::new();
    let mut loaded = LOADED.get_or_init(Default::default).lock().unwrap();
    loaded.entry(code.to_string()).or_insert_with(|| Arc::new(read_dir(&fixture_dir(code)).unwrap())).clone()
}

/// A test pack copied into a data directory, as an installed pack.
#[cfg(test)]
pub(crate) fn install_fixture(data_dir: &Path, code: &str) {
    let to = data_dir.join("languages").join(code);
    std::fs::create_dir_all(&to).unwrap();
    for entry in std::fs::read_dir(fixture_dir(code)).unwrap().flatten() {
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("legejo-langpack-{}", crate::books::new_uuid()))
    }

    #[test]
    fn no_pack_is_built_in() {
        let packs = Packs::new(temp_dir());
        for code in ["es", "fr", "de", "eo"] {
            assert!(packs.get(code).is_none(), "{code}");
        }
        assert!(packs.installed().is_empty());
        assert!(packs.aliases().is_empty());
    }

    #[test]
    fn an_installed_spanish_pack_finds_irregular_forms() {
        let dir = temp_dir();
        install_fixture(&dir, "es");
        let packs = Packs::new(dir.clone());
        let es = packs.get("es").unwrap();
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
        assert_eq!(codes, ["es"]);
        assert!(packs.aliases().contains(&("spanish".to_string(), "es".to_string())), "{:?}", packs.aliases());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn french_and_german_packs_and_german_nouns_keep_their_capital() {
        let word = |p: &Pack, form: &str| p.lemmas_of(form).iter().map(|n| p.lemma(*n).unwrap().word.to_string()).collect::<Vec<_>>();
        let fr = fixture("fr");
        assert!(word(&fr, "était").contains(&"être".to_string()));
        assert_eq!(word(&fr, "allait"), ["aller"]);
        let de = fixture("de");
        assert_eq!(word(&de, "ging"), ["gehen"]);
        assert!(word(&de, "häuser").contains(&"haus".to_string()));
        assert_eq!(de.find("haus").unwrap().written, "Haus");
        assert_eq!(de.find("gehen").unwrap().written, "gehen");
    }

    #[test]
    fn a_text_only_pack_is_built_at_load() {
        let dir = temp_dir();
        write_xx(&dir.join("languages/xx"), "to run", "1");
        let packs = Packs::new(dir.clone());
        let xx = packs.get("xx").unwrap();
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

        // A pack appears on the disk later: there once reloaded.
        assert!(packs.get("es").is_none());
        install_fixture(&dir, "es");
        assert!(packs.get("es").is_none(), "remembered as missing until reloaded");
        assert!(!packs.reload("es").unwrap().lemmas_of("hizo").is_empty());

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

    #[test]
    fn extras_are_read_when_asked_for_and_are_not_part_of_the_packs_id() {
        let dir = temp_dir();
        let xx_dir = dir.join("languages/xx");
        write_xx(&xx_dir, "to run", "1");
        let packs = Packs::new(dir.clone());
        let bare = packs.get("xx").unwrap();
        assert!(bare.extras().is_none() && !bare.has_extras());
        assert_eq!(bare.id_with_extras(), bare.id());

        write_xx_extras(&xx_dir);
        let xx = packs.reload("xx").unwrap();
        assert_eq!(xx.id(), bare.id(), "the words are not counted again for extras");
        assert_ne!(xx.id_with_extras(), xx.id());
        assert!(xx.has_extras());
        let extras = xx.extras().unwrap();
        assert!(extras.has_tags() && extras.has_frequency());
        assert_eq!(extras.meta.frequency.as_deref(), Some("A word list, CC0"));
        assert_eq!(extras.tags_of("blorp", "blorped"), [["past"]]);
        assert_eq!(extras.tags_of("blorp", "flib"), [vec!["third-person", "singular", "past"], vec!["first-person", "singular", "past"]]);
        assert!(extras.tags_of("flib", "flib").is_empty() && extras.tags_of("blorp", "nothing").is_empty());
        let forms: Vec<String> = extras.forms_of("blorp").into_iter().map(|(f, _)| f).collect();
        assert_eq!(forms, ["blorped", "blorping", "flib"]);
        assert!(extras.forms_of("blor").is_empty() && extras.forms_of("zzz").is_empty());
        // The same lines in the builder's order (by lemma, then form) are
        // read straight into the index, and say the same.
        let unordered = std::fs::read_to_string(xx_dir.join("tags.tsv")).unwrap();
        let mut ordered: Vec<&str> = unordered.lines().filter(|l| l.matches('\t').count() == 2).collect();
        ordered.sort_by_key(|l| {
            let mut parts = l.split('\t');
            let form = parts.next().unwrap();
            (parts.next().unwrap(), form)
        });
        let other = dir.join("languages/yy");
        write_xx(&other, "to run", "1");
        std::fs::write(other.join("tags.tsv"), ordered.join("\n") + "\n").unwrap();
        let (mut hash, mut kinds) = (Sha256::new(), Vec::new());
        let index = read_tags(&other.join("tags.tsv"), &mut hash, &mut kinds).unwrap();
        assert_eq!(index.len(), 4);
        assert_eq!(kinds[index.get("blorp\tblorped").unwrap() as usize], "past");
        assert_eq!(kinds[index.get("zam\tzams").unwrap() as usize], "plural");
        // Ranks by the pack's lemmas: a word it does not have takes no rank.
        let rank = |w: &str| extras.rank(xx.line_of(w).unwrap());
        assert_eq!((rank("ko"), rank("blorp"), rank("zam"), rank("flib")), (Some(1), Some(2), Some(3), None));
        // The checked import refuses extras that cannot be read.
        assert!(check_dir(&xx_dir, "xx").is_ok());
        std::fs::write(xx_dir.join("extras.json"), "not json").unwrap();
        assert!(check_dir(&xx_dir, "xx").err().map(|e| format!("{e:#}")).unwrap_or_default().contains("extras.json"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
