//! Build a language pack from kaikki.org's extract of the English Wiktionary
//! (https://kaikki.org/dictionary/<Language>/), which gives each word of a
//! language with English meanings and its inflected forms:
//!
//!     langpack <code> "<Name>" <kaikki.jsonl.gz> <out dir> [--index]
//!
//! writes `pack.json`, `lemmas.tsv` (lemma, parts of speech, English and,
//! when it is written otherwise, the lemma as written) and `forms.tsv`
//! (form, the lemmas it can be) into the directory, then prints a summary of
//! what came out. With `--index` it writes `forms.fst` and `ambiguous.tsv`
//! instead of `forms.tsv`: the index the server otherwise builds when it
//! loads a pack, kept pre-built for the packs compiled into the binary
//! (src/langpack.rs reads them). tools/langpack/README.md says how a pack
//! is used and edited by hand.
//!
//! The extraction is a set of small rules, each a function with a test on
//! the fixture at the end of this file: which entries and senses to skip
//! (abbreviations, letter names), which are forms of another word rather
//! than words of their own, and how to shorten Wiktionary's senses into one
//! line of English per lemma.

use anyhow::{bail, Context};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const FORMAT: u32 = 1;
/// A form's value in the fst: a lemma's line number, or with this bit set
/// the number of a line in ambiguous.tsv.
const AMBIGUOUS: u64 = 1 << 40;
/// The longest gloss written for a lemma, in characters.
const MAX_GLOSS: usize = 110;
/// A gloss longer than this gives way to a shorter sense of the same part
/// of speech in the first slot, and is cut at a separator after `CUT_AT`.
const LONG_GLOSS: usize = 60;
const CUT_AT: usize = 30;
const SENSES_PER_POS: usize = 3;
const PARTS_PER_LEMMA: usize = 2;

/// Parts of speech that never give a lemma: names, phrases, affixes.
const SKIP_POS: &[&str] = &[
    "name", "proverb", "phrase", "prefix", "suffix", "infix", "interfix", "circumfix", "character", "symbol", "abbrev", "romanization", "punct",
];
/// Parts of speech of little words: shown first when a word is one of them
/// too, and never swallowed as a form of a noun. Interjections are not
/// here: most are a noun or adjective first ("hombre", "largo"), so they
/// trail instead.
const LITTLE: &[&str] = &["article", "prep", "conj", "pron", "det", "adv", "particle", "contraction", "num"];
/// The closed classes among them: a word that is one keeps its line to
/// them (rule 6: "nous" is not also the philosophers' nous), where an
/// adverb lets its adjective and noun follow ("medio", "derecho").
const CLOSED: &[&str] = &["article", "prep", "conj", "pron", "det", "particle", "contraction", "num"];
/// Words that may precede "form of" in a gloss that names a variant of the
/// same word ("Separated form of nach-", "short form of").
const FORM_WORDS: &[&str] = &[
    "separated", "separable", "short", "long", "reduced", "contracted", "clitic", "enclitic", "proclitic", "unstressed", "stressed",
    "emphatic", "elided", "weak", "strong", "full",
];
/// Words of a description of the word that may precede its meaning after a
/// colon ("neuter definite article used before adjectives: the").
const DESCRIBES: &[&str] = &[
    "pronoun", "article", "adjective", "adverb", "verb", "noun", "particle", "preposition", "conjunction", "determiner", "numeral",
    "interjection", "used", "form", "expression", "word", "marker", "suffix", "prefix",
];
/// Words of a note on an inflection inside a gloss ("dative of él").
const CASES: &[&str] = &[
    "nominative", "accusative", "dative", "genitive", "vocative", "ablative", "locative", "instrumental", "prepositional", "partitive",
    "singular", "plural", "feminine", "masculine", "neuter", "comparative", "superlative", "reflexive", "inflection", "inflected", "form",
    "participle", "infinitive", "gerund", "imperative", "subjunctive", "indicative", "present", "past", "preterite", "imperfect", "future",
    "first", "second", "third", "diminutive", "augmentative",
];
/// Inflection-table rows that are not forms.
const BAD_FORM_TAGS: &[&str] = &["table-tags", "inflection-template", "romanization", "class", "auxiliary", "obsolete", "archaic"];
const ABBREVIATION_TAGS: &[&str] = &["abbreviation", "initialism", "acronym"];
/// Senses used only when a lemma has no other.
const HARD_TAGS: &[&str] = &["obsolete", "archaic", "rare"];
/// Senses sorted after plain ones (regional tags, which kaikki capitalises,
/// and a sense's topics count too).
const SOFT_TAGS: &[&str] = &[
    "formal", "literary", "colloquial", "slang", "vulgar", "dialectal", "historical", "dated", "obsolete", "archaic", "uncommon", "rare",
    "nonstandard", "neologism", "humorous", "derogatory", "offensive", "informal", "euphemistic", "childish", "poetic", "regional",
    "proscribed", "text-messaging", "eye-dialect",
];
/// Alternative spellings that are heard rather than read: not linked to the
/// word they stand for (Quebec "a" for "elle"), unlike obsolete spellings.
const SPOKEN_TAGS: &[&str] = &[
    "colloquial", "slang", "dialectal", "nonstandard", "informal", "text-messaging", "eye-dialect", "humorous", "proscribed", "vulgar",
    "childish", "pronunciation-spelling",
];
/// Words that may precede "form of"/"spelling of" in a gloss that gives an
/// alternative spelling without kaikki's `alt-of` tag.
const ALT_QUALIFIERS: &[&str] = &[
    "alternative", "alt", "obsolete", "archaic", "dated", "former", "rare", "nonstandard", "pronunciation", "eye-dialect", "eye", "dialect",
    "informal", "colloquial", "apocopic", "apocopated", "aphetic", "apheretic", "superseded", "standard", "old", "traditional", "variant",
    "unofficial", "common", "less", "uncommon", "poetic", "respelling",
];
/// A gloss made only of these words describes an inflection, not a meaning
/// ("nominative/accusative feminine singular").
const GRAMMAR: &[&str] = &[
    "nominative", "accusative", "dative", "genitive", "vocative", "ablative", "locative", "instrumental", "prepositional", "partitive",
    "singular", "plural", "dual", "collective", "feminine", "masculine", "neuter", "common", "gender", "epicene", "first", "second",
    "third", "person", "present", "past", "future", "preterite", "imperfect", "perfect", "pluperfect", "conditional", "indicative",
    "subjunctive", "imperative", "participle", "gerund", "infinitive", "supine", "aorist", "strong", "weak", "mixed", "definite",
    "indefinite", "comparative", "superlative", "positive", "degree", "form", "forms", "inflection", "inflected", "of", "and", "or",
    "with", "without", "reflexive", "object", "subject", "possessive", "demonstrative", "relative", "interrogative", "personal",
    "polite", "informal", "formal", "negative", "affirmative", "active", "passive", "voice", "mood", "tense", "case", "class", "noun",
    "verb", "adjective", "adverb", "pronoun", "determiner", "article", "stem", "base", "variant", "short", "long", "attributive",
    "predicative", "all", "emphatic", "stressed", "unstressed", "contracted", "uncontracted", "elided", "simple", "compound", "i", "ii",
    "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii", "xiii", "xiv", "xv", "xvi", "xvii", "xviii",
];
/// Other names a pack answers to (ISO 639-2/3 codes, English and native
/// names), for languages where they are known; the dump's own English name
/// and the given name are added at build time.
const ALIASES: &[(&str, &[&str])] = &[
    ("es", &["spa", "spanish", "español", "castellano", "castilian"]),
    ("fr", &["fra", "fre", "french", "français"]),
    ("de", &["deu", "ger", "german", "deutsch"]),
    ("sw", &["swa", "swahili", "kiswahili"]),
    ("it", &["ita", "italian", "italiano"]),
    ("pt", &["por", "portuguese", "português"]),
    ("nl", &["nld", "dut", "dutch", "nederlands"]),
    ("sv", &["swe", "swedish", "svenska"]),
    ("da", &["dan", "danish", "dansk"]),
    ("no", &["nor", "nob", "nno", "norwegian", "norsk", "bokmål", "nynorsk"]),
    ("fi", &["fin", "finnish", "suomi"]),
    ("is", &["isl", "ice", "icelandic", "íslenska"]),
    ("pl", &["pol", "polish", "polski"]),
    ("cs", &["ces", "cze", "czech", "čeština"]),
    ("ru", &["rus", "russian", "русский"]),
    ("uk", &["ukr", "ukrainian", "українська"]),
    ("el", &["ell", "gre", "greek", "ελληνικά"]),
    ("la", &["lat", "latin", "latina"]),
    ("tr", &["tur", "turkish", "türkçe"]),
    ("hu", &["hun", "hungarian", "magyar"]),
    ("ro", &["ron", "rum", "romanian", "română"]),
    ("ca", &["cat", "catalan", "català"]),
    ("ga", &["gle", "irish", "gaeilge"]),
    ("cy", &["cym", "wel", "welsh", "cymraeg"]),
    ("ja", &["jpn", "japanese", "日本語"]),
    ("zh", &["zho", "chi", "chinese", "中文"]),
    ("ar", &["ara", "arabic", "العربية"]),
    ("hi", &["hin", "hindi", "हिन्दी"]),
    ("yo", &["yor", "yoruba", "yorùbá"]),
    ("zu", &["zul", "zulu", "isizulu"]),
];

// ---------------------------------------------------------------- kaikki

#[derive(Deserialize, Default)]
struct Entry {
    #[serde(default)]
    word: String,
    #[serde(default)]
    pos: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    lang_code: String,
    #[serde(default)]
    senses: Vec<Sense>,
    #[serde(default)]
    forms: Vec<Form>,
}

#[derive(Deserialize, Default)]
struct Sense {
    #[serde(default)]
    glosses: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    qualifier: Option<String>,
    #[serde(default)]
    form_of: Vec<Target>,
    #[serde(default)]
    alt_of: Vec<Target>,
}

#[derive(Deserialize, Default)]
struct Target {
    #[serde(default)]
    word: String,
    #[serde(default)]
    extra: String,
}

#[derive(Deserialize, Default)]
struct Form {
    #[serde(default)]
    form: String,
    #[serde(default)]
    tags: Vec<String>,
}

impl Sense {
    fn has_tag(&self, tags: &[&str]) -> bool {
        self.tags.iter().any(|t| tags.contains(&t.as_str()))
    }

    /// The words of the qualifier ("archaic or royal"), which carries what
    /// the tags sometimes do not.
    fn qualifier_words(&self) -> impl Iterator<Item = String> + '_ {
        self.qualifier.as_deref().unwrap_or("").split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase())
    }

    /// Used only when the lemma has no other sense.
    fn is_hard(&self) -> bool {
        self.has_tag(HARD_TAGS) || self.qualifier_words().any(|w| HARD_TAGS.contains(&w.as_str()))
    }

    /// Sorted after plain senses: registers, regions, fields.
    fn is_soft(&self) -> bool {
        self.has_tag(SOFT_TAGS)
            || self.tags.iter().any(|t| t.starts_with(|c: char| c.is_ascii_uppercase()))
            || !self.topics.is_empty()
            || self.qualifier_words().any(|w| SOFT_TAGS.contains(&w.as_str()))
    }

    /// The sense's English, cleaned: the most specific of nested glosses,
    /// without wikitext, parentheses and a final stop, and without a
    /// grammatical description before a colon ("A reflexive pronoun:
    /// oneself, himself").
    fn gloss(&self) -> String {
        let g = self.glosses.last().map(|g| short(&strip_wikitext(g))).unwrap_or_default();
        after_description(&g).unwrap_or(g)
    }
}

/// The meaning after "<description of the word>: ", when the gloss has one.
fn after_description(gloss: &str) -> Option<String> {
    let (before, after) = gloss.split_once(": ")?;
    let describes = before.split_whitespace().count() >= 3
        && before.to_lowercase().split(|c: char| !c.is_alphabetic()).any(|w| DESCRIBES.contains(&w));
    (describes && !after.trim().is_empty()).then(|| after.trim().to_string())
}

// ---------------------------------------------------------------- the rules

/// Wikitext kaikki leaves in some glosses: `[[target|text]]` and `[[text]]`
/// become the text, `{{…}}`, `((…))` (with a joining underscore) and
/// `[bracketed notes]` go.
fn strip_wikitext(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(['[', '{', '(']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let (open, close) = if tail.starts_with("[[") {
            ("[[", "]]")
        } else if tail.starts_with("{{") {
            ("{{", "}}")
        } else if tail.starts_with("((") {
            ("((", "))")
        } else if tail.starts_with('[') {
            ("[", "]")
        } else {
            out.push('(');
            rest = &tail[1..];
            continue;
        };
        let Some(end) = tail[open.len()..].find(close) else {
            // Left open (kaikki truncates some glosses): keep the words.
            out.push_str(&tail[open.len()..]);
            return out.replace(['[', ']', '{', '}'], "");
        };
        let inner = &tail[open.len()..open.len() + end];
        match open {
            "[[" => out.push_str(&strip_wikitext(inner.rsplit('|').next().unwrap_or(inner))),
            "((" => {
                if out.ends_with('_') {
                    out.pop();
                }
            }
            _ => {}
        }
        rest = &tail[open.len() + end + close.len()..];
    }
    out.push_str(rest);
    out
}

/// A gloss without its parentheses, doubled spaces and final punctuation.
fn short(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let mut tidy = String::with_capacity(out.len());
    for c in out.chars() {
        if c.is_whitespace() {
            if !tidy.ends_with(' ') {
                tidy.push(' ');
            }
        } else {
            if matches!(c, ',' | ';' | ':' | '.') && tidy.ends_with(' ') {
                tidy.pop();
            }
            tidy.push(c);
        }
    }
    tidy.trim().trim_end_matches(['.', ':', ';', ',', ' ']).to_string()
}

/// A lemma key: lower case, without the hyphens that mark a stem (`-sema`),
/// one plain word (no spaces, digits or dots).
fn key_of(word: &str) -> Option<String> {
    let w = word.trim().trim_matches('-').to_lowercase();
    if w.is_empty() || !w.chars().all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-')) {
        return None;
    }
    Some(w)
}

/// The key of a word a gloss points at, with the punctuation around it.
fn target_key(token: &str) -> Option<String> {
    key_of(token.trim_matches(|c: char| !c.is_alphanumeric()))
}

/// Rule 1: initialisms and the like are not words to learn.
fn is_abbreviation(sense: &Sense, gloss: &str) -> bool {
    let g = gloss.to_lowercase();
    sense.has_tag(ABBREVIATION_TAGS) || ["initialism of", "abbreviation of", "acronym of"].iter().any(|p| g.starts_with(p))
}

/// Rule 8: the name of a letter is not a word of the language.
fn is_letter(sense: &Sense, gloss: &str) -> bool {
    if sense.has_tag(&["letter"]) {
        return true;
    }
    let g = gloss.to_lowercase();
    if let Some(i) = g.find("name of the") {
        if g[i..].contains("letter") {
            return true;
        }
    }
    let padded = format!(" {g} ");
    padded.find(" letter ").is_some_and(|i| {
        let rest = &padded[i + 8..];
        // "the Hebrew letter ה": the letter itself follows.
        rest.contains("alphabet") || rest.contains("script") || rest.split_whitespace().next().is_some_and(|w| w.chars().count() == 1)
    })
}

/// Rule 11: "inflected form of X" said in the gloss alone, as Swahili entries
/// do, optionally after the noun classes ("m class/wa class inflected form
/// of -ake"). Not "a form of address", "female form of Gatte".
fn form_of_in_gloss(gloss: &str) -> Option<String> {
    let lower = gloss.to_lowercase();
    for head in ["inflected form and adverbial form of ", "inflected form of ", "inflection of ", "form of "] {
        if let Some(i) = lower.find(head) {
            let prefix = lower[..i].trim();
            let classes = prefix.contains("class") && prefix.chars().all(|c| c.is_alphabetic() || matches!(c, ' ' | '-' | '/'));
            let variant = !prefix.is_empty() && prefix.split_whitespace().all(|w| FORM_WORDS.contains(&w));
            if prefix.is_empty() || classes || variant {
                return target_key(lower[i + head.len()..].split_whitespace().next()?);
            }
            return None;
        }
    }
    None
}

/// Rule 7: the words an alternative-spelling sense stands for, from kaikki's
/// `alt_of` or from a gloss like "obsolete spelling of X".
fn alt_targets(sense: &Sense, gloss: &str) -> Vec<String> {
    if sense.has_tag(&["alt-of"]) {
        return sense.alt_of.iter().filter_map(|t| target_key(&t.word)).collect();
    }
    let lower = gloss.to_lowercase();
    for head in ["misspelling of ", "spelling of ", "form of "] {
        if let Some(i) = lower.find(head) {
            let qualifiers = lower[..i].split_whitespace().all(|w| ALT_QUALIFIERS.contains(&w) || w.starts_with("pre-") || w.starts_with("post-"));
            if qualifiers && !lower[..i].trim().is_empty() {
                return lower[i + head.len()..].split_whitespace().next().and_then(target_key).into_iter().collect();
            }
            return Vec::new();
        }
    }
    Vec::new()
}

/// Rule 2: what a little word that Wiktionary files as a form of another
/// ("me": accusative of yo) means by itself, from `form_of[].extra` or the
/// gloss after the colon; None when the gloss only names the inflection.
fn little_form_gloss(sense: &Sense, gloss: &str) -> Option<String> {
    for t in &sense.form_of {
        let x = short(&strip_wikitext(&t.extra));
        if x.starts_with(|c: char| c.is_alphabetic()) && !is_grammar_only(&x) {
            return Some(x);
        }
    }
    let candidate = if let Some(i) = gloss.find(": ") {
        &gloss[i + 2..]
    } else if gloss.to_lowercase().contains(" of ") {
        &gloss[gloss.rfind("; ")? + 2..]
    } else if sense.glosses.len() > 1 {
        gloss
    } else {
        return None;
    };
    let candidate = candidate.trim();
    (!candidate.is_empty() && !is_grammar_only(candidate)).then(|| candidate.to_string())
}

/// Whether a text only describes an inflection ("genitive plural").
fn is_grammar_only(s: &str) -> bool {
    let mut any = false;
    for w in s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        any = true;
        let w = w.to_lowercase();
        if !GRAMMAR.contains(&w.as_str()) && !w.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    any
}

/// Rule 4: little words and verbs lead a lemma's parts of speech,
/// interjections trail, the rest keep Wiktionary's order in between.
fn rank(pos: &str) -> u8 {
    if pos == "verb" || LITTLE.contains(&pos) {
        0
    } else if pos == "intj" {
        2
    } else {
        1
    }
}

/// "dative of él" inside a gloss: a note on the inflection, not a meaning.
fn is_inflection_note(atom: &str) -> bool {
    let lower = atom.to_lowercase();
    match lower.split_once(" of ") {
        Some((left, right)) => {
            let right = right.trim();
            is_grammar_only(left) && left.split(|c: char| !c.is_alphanumeric()).any(|w| CASES.contains(&w)) && !right.is_empty() && !right.contains(' ')
        }
        None => false,
    }
}

// ---------------------------------------------------------------- glosses

/// A piece of a gloss between separators, with the separator before it.
#[derive(Clone, Debug, PartialEq)]
struct Atom {
    text: String,
    sep: char,
}

/// Rule 5: a gloss split at `;`, `,` (not inside a number) and ` / `
/// ("grey / gray"), which the pack uses between parts of speech.
fn atoms(text: &str) -> Vec<Atom> {
    let chars: Vec<char> = text.chars().collect();
    let (mut out, mut buf, mut sep) = (Vec::new(), String::new(), ';');
    for (i, &c) in chars.iter().enumerate() {
        let in_number = c == ',' && i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit());
        let slash = c == '/' && i > 0 && chars[i - 1] == ' ' && chars.get(i + 1) == Some(&' ');
        if (matches!(c, ';' | ',') && !in_number) || slash {
            let t = buf.trim();
            if !t.is_empty() {
                out.push(Atom { text: t.to_string(), sep });
            }
            buf.clear();
            sep = if slash { ',' } else { c };
        } else {
            buf.push(c);
        }
    }
    let t = buf.trim();
    if !t.is_empty() {
        out.push(Atom { text: t.to_string(), sep });
    }
    out
}

fn render(atoms: &[Atom]) -> String {
    let mut s = String::new();
    for (i, a) in atoms.iter().enumerate() {
        if i > 0 {
            s.push(a.sep);
            s.push(' ');
        }
        s.push_str(&a.text);
    }
    s
}

fn width(atoms: &[Atom]) -> usize {
    atoms.iter().enumerate().map(|(i, a)| a.text.chars().count() + if i > 0 { 2 } else { 0 }).sum()
}

/// Rule 9: a long first gloss is cut at the first separator after CUT_AT.
fn cut_long(atoms: &mut Vec<Atom>) {
    let mut keep = 0;
    while keep < atoms.len() && width(&atoms[..keep]) < CUT_AT {
        keep += 1;
    }
    atoms.truncate(keep.max(1));
}

/// Rule 9: the whole line within MAX_GLOSS, giving up trailing atoms of the
/// longest part first and cutting a single atom at a word boundary last.
fn fit(parts: &mut [Vec<Atom>]) {
    loop {
        let total: usize = parts.iter().map(|p| width(p)).sum::<usize>() + 3 * parts.len().saturating_sub(1);
        if total <= MAX_GLOSS {
            return;
        }
        let longest = (0..parts.len()).filter(|&i| parts[i].len() > 1).max_by_key(|&i| width(&parts[i]));
        if let Some(i) = longest {
            parts[i].pop();
            continue;
        }
        let Some(i) = (0..parts.len()).max_by_key(|&i| width(&parts[i])) else { return };
        let Some(atom) = parts[i].first_mut() else { return };
        let want = atom.text.chars().count().saturating_sub(total - MAX_GLOSS + 1).max(1);
        let cut: String = atom.text.chars().take(want).collect();
        let at = cut.rfind(' ').filter(|&at| at > 0).unwrap_or(cut.len());
        atom.text = format!("{}…", cut[..at].trim_end());
        return;
    }
}

/// One sense's gloss with its standing.
#[derive(Clone, Debug)]
struct Gloss {
    atoms: Vec<Atom>,
    soft: bool,
    hard: bool,
}

impl Gloss {
    fn new(text: String, sense: &Sense) -> Option<Gloss> {
        let mut atoms = atoms(&text);
        atoms.retain(|a| !is_inflection_note(&a.text));
        if let Some(first) = atoms.first_mut() {
            first.sep = ';';
        }
        (!atoms.is_empty()).then(|| Gloss { atoms, soft: sense.is_soft(), hard: sense.is_hard() })
    }
}

#[derive(Default)]
struct LemmaInfo {
    /// Parts of speech in the order met, each with its senses.
    pos: Vec<(String, Vec<Gloss>)>,
    /// A capitalised spelling of the word, if no entry writes it lower-case.
    written: Option<String>,
    lower_seen: bool,
}

/// A line of lemmas.tsv.
#[derive(Debug, Clone, PartialEq)]
struct Line {
    key: String,
    pos: String,
    gloss: String,
    written: String,
    little: bool,
    senses: usize,
}

/// Rules 4, 5, 6 and 9: a lemma's parts of speech and one line of English.
fn compose(key: &str, info: &LemmaInfo) -> Option<Line> {
    let mut order: Vec<usize> = (0..info.pos.len()).collect();
    order.sort_by_key(|&i| rank(&info.pos[i].0));
    let hard_only = info.pos.iter().all(|(_, g)| g.iter().all(|g| g.hard));
    let (mut names, mut parts, mut seen) = (Vec::new(), Vec::<Vec<Atom>>::new(), HashSet::new());
    let (mut first_closed, mut senses, mut little) = (false, 0, false);
    for i in order {
        let (pos, glosses) = &info.pos[i];
        let mut ss: Vec<&Gloss> = glosses.iter().filter(|g| hard_only || !g.hard).collect();
        if ss.is_empty() {
            continue;
        }
        ss.sort_by_key(|g| g.soft);
        let is_little = LITTLE.contains(&pos.as_str());
        names.push(pos.clone());
        senses += ss.len();
        little |= is_little;
        if parts.len() >= PARTS_PER_LEMMA {
            continue;
        }
        // Rule 6: after a closed-class word's meaning only other little
        // parts; a part with only marked senses adds nothing to a line
        // that has one.
        if !parts.is_empty() && ((first_closed && !is_little) || (!hard_only && ss.iter().all(|g| g.soft))) {
            continue;
        }
        if width(&ss[0].atoms) > LONG_GLOSS {
            if let Some(j) = ss.iter().position(|g| width(&g.atoms) <= LONG_GLOSS) {
                let g = ss.remove(j);
                ss.insert(0, g);
            }
        }
        ss.truncate(SENSES_PER_POS);
        let mut part = Vec::new();
        for (k, g) in ss.iter().enumerate() {
            let mut a = g.atoms.clone();
            if k == 0 && width(&a) > LONG_GLOSS {
                cut_long(&mut a);
            }
            a[0].sep = ';';
            part.extend(a);
        }
        // Rule 5: an atom said once per line; a sense whose first atom goes
        // still starts with its own separator.
        let (mut kept, mut pending) = (Vec::new(), false);
        for mut a in part {
            if seen.insert(a.text.to_lowercase()) {
                if pending {
                    a.sep = ';';
                    pending = false;
                }
                kept.push(a);
            } else if a.sep == ';' {
                pending = true;
            }
        }
        if kept.is_empty() {
            continue;
        }
        if parts.is_empty() {
            first_closed = CLOSED.contains(&pos.as_str());
        }
        parts.push(kept);
    }
    if parts.is_empty() {
        return None;
    }
    fit(&mut parts);
    let gloss = parts.iter().map(|p| render(p)).collect::<Vec<_>>().join(" / ");
    let written = if info.lower_seen { String::new() } else { info.written.clone().unwrap_or_default() };
    Some(Line { key: key.to_string(), pos: names.join(","), gloss, written, little, senses })
}

// ---------------------------------------------------------------- building

/// How a form came to point at a lemma.
#[derive(Clone, Copy, Default)]
struct Kind {
    /// From the lemma's inflection table.
    table: bool,
    /// Said by the form's own entry ("hizo": inflection of hacer), by an
    /// entry that is not a noun or interjection (rule 3).
    stated: bool,
}

#[derive(Default)]
struct Stats {
    entries: u64,
    parse_errors: u64,
    first_error: String,
    all_caps: u64,
    abbreviations: u64,
    letters: u64,
    table_links: u64,
    stated_links: u64,
    alt_links: u64,
    gloss_links: u64,
    dropped_from_little: u64,
}

#[derive(Default)]
struct Builder {
    keys: Vec<String>,
    key_index: HashMap<String, u32>,
    lemmas: HashMap<u32, LemmaInfo>,
    /// form -> (lemma key id, how)
    forms: HashMap<String, Vec<(u32, Kind)>>,
    stats: Stats,
    lang: String,
    lang_code: String,
}

/// The result: lemma lines in byte order of their keys, and every form with
/// the line numbers of the lemmas it can be.
struct Pack {
    lines: Vec<Line>,
    forms: BTreeMap<String, Vec<u32>>,
    stats: Stats,
    /// The language's English name, as the dump says.
    lang: String,
    lang_code: String,
}

impl Builder {
    fn intern(&mut self, key: &str) -> u32 {
        if let Some(&id) = self.key_index.get(key) {
            return id;
        }
        let id = self.keys.len() as u32;
        self.keys.push(key.to_string());
        self.key_index.insert(key.to_string(), id);
        id
    }

    fn link(&mut self, form: &str, target: u32, kind: Kind) {
        let links = match self.forms.get_mut(form) {
            Some(l) => l,
            None => self.forms.entry(form.to_string()).or_default(),
        };
        match links.iter_mut().find(|(t, _)| *t == target) {
            Some((_, k)) => {
                k.table |= kind.table;
                k.stated |= kind.stated;
            }
            None => links.push((target, kind)),
        }
    }

    fn add(&mut self, e: &Entry) {
        self.stats.entries += 1;
        if self.lang.is_empty() {
            self.lang = e.lang.clone();
            self.lang_code = e.lang_code.clone();
        }
        if SKIP_POS.contains(&e.pos.as_str()) {
            return;
        }
        let word = e.word.trim().trim_matches('-');
        let Some(key) = key_of(word) else { return };
        if word.chars().count() > 1 && word != key && word == word.to_uppercase() {
            self.stats.all_caps += 1;
            return;
        }
        let little = LITTLE.contains(&e.pos.as_str());
        let noun = matches!(e.pos.as_str(), "noun" | "intj");
        let (mut glosses, mut links, mut alts) = (Vec::new(), Vec::new(), Vec::new());
        for s in &e.senses {
            let gloss = s.gloss();
            if is_abbreviation(s, &gloss) {
                self.stats.abbreviations += 1;
                continue;
            }
            if is_letter(s, &gloss) {
                self.stats.letters += 1;
                continue;
            }
            if !s.form_of.is_empty() {
                if little {
                    if let Some(g) = little_form_gloss(s, &gloss).and_then(|g| Gloss::new(g, s)) {
                        glosses.push(g);
                        continue;
                    }
                    if s.is_hard() {
                        continue;
                    }
                }
                links.extend(s.form_of.iter().filter_map(|t| target_key(&t.word)));
                continue;
            }
            let targets = alt_targets(s, &gloss);
            if s.has_tag(&["alt-of"]) || !targets.is_empty() {
                if !s.has_tag(SPOKEN_TAGS) {
                    alts.extend(targets);
                }
                continue;
            }
            if let Some(t) = form_of_in_gloss(&gloss) {
                self.stats.gloss_links += 1;
                links.push(t);
                continue;
            }
            if let Some(g) = Gloss::new(gloss, s) {
                glosses.push(g);
            }
        }
        let id = self.intern(&key);
        self.stats.stated_links += links.len() as u64;
        self.stats.alt_links += alts.len() as u64;
        for t in links.into_iter().chain(alts) {
            let target = self.intern(&t);
            self.link(&key, target, Kind { table: false, stated: !noun });
        }
        if glosses.is_empty() {
            return;
        }
        let info = self.lemmas.entry(id).or_default();
        // Rule 13: written with a capital only when no entry writes the
        // word lower-case (German "Haus", but "essen" for "Essen" too).
        if word == key {
            info.lower_seen = true;
        } else if !info.lower_seen && info.written.is_none() {
            info.written = Some(word.to_string());
        }
        match info.pos.iter_mut().find(|(p, _)| *p == e.pos) {
            Some((_, g)) => g.extend(glosses),
            None => info.pos.push((e.pos.clone(), glosses)),
        }
        self.link(&key, id, Kind { table: false, stated: true });
        for f in &e.forms {
            if f.tags.iter().any(|t| BAD_FORM_TAGS.contains(&t.as_str())) {
                continue;
            }
            // Rule 12: a form with a space (German "kommt an") cannot be looked up.
            let Some(form) = key_of(&f.form) else { continue };
            self.stats.table_links += 1;
            self.link(&form, id, Kind { table: true, stated: false });
        }
    }

    fn finish(self) -> Pack {
        let Builder { keys, key_index, lemmas, forms, mut stats, lang, lang_code } = self;
        let mut lines: Vec<(u32, Line)> = lemmas.iter().filter_map(|(id, info)| compose(&keys[*id as usize], info).map(|l| (*id, l))).collect();
        lines.sort_by(|a, b| a.1.key.as_bytes().cmp(b.1.key.as_bytes()));
        let line_of: HashMap<u32, u32> = lines.iter().enumerate().map(|(n, (id, _))| (*id, n as u32)).collect();
        let little: HashSet<u32> = lines.iter().filter(|(_, l)| l.little).map(|(id, _)| *id).collect();
        let mut out = BTreeMap::new();
        for (form, links) in forms {
            let own = key_index.get(&form).copied();
            // Rule 3: a little word is its own lemma; only a verb or
            // adjective that says it is also its form adds to that.
            let own_little = own.is_some_and(|id| little.contains(&id));
            let mut numbers: Vec<u32> = links
                .iter()
                .filter_map(|(target, kind)| {
                    let n = *line_of.get(target)?;
                    if own == Some(*target) {
                        return Some(n);
                    }
                    if own_little && !kind.stated {
                        stats.dropped_from_little += 1;
                        return None;
                    }
                    Some(n)
                })
                .collect();
            numbers.sort_unstable();
            numbers.dedup();
            if !numbers.is_empty() {
                out.insert(form, numbers);
            }
        }
        Pack { lines: lines.into_iter().map(|(_, l)| l).collect(), forms: out, stats, lang, lang_code }
    }
}

/// Read a kaikki dump, one JSON entry per line.
fn build(reader: impl BufRead) -> anyhow::Result<Pack> {
    let mut b = Builder::default();
    for (n, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", n + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Entry>(&line) {
            Ok(e) => b.add(&e),
            Err(e) => {
                b.stats.parse_errors += 1;
                if b.stats.first_error.is_empty() {
                    b.stats.first_error = format!("line {}: {e}", n + 1);
                }
            }
        }
    }
    if b.stats.entries == 0 {
        bail!("no entries found{}", if b.stats.first_error.is_empty() { String::new() } else { format!(" ({})", b.stats.first_error) });
    }
    if b.stats.parse_errors * 100 > b.stats.entries {
        bail!("{} of {} lines are not kaikki entries ({})", b.stats.parse_errors, b.stats.entries + b.stats.parse_errors, b.stats.first_error);
    }
    Ok(b.finish())
}

// ---------------------------------------------------------------- writing

fn aliases(code: &str, name: &str, lang: &str) -> Vec<String> {
    let mut out: Vec<String> = ALIASES.iter().find(|(c, _)| *c == code).map(|(_, a)| a.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    for extra in [lang.trim().to_lowercase(), name.trim().to_lowercase()] {
        if !extra.is_empty() && extra != code && !out.contains(&extra) {
            out.push(extra);
        }
    }
    out
}

fn today() -> String {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::OffsetDateTime::now_utc().date().format(&format).unwrap_or_default()
}

fn write_meta(dir: &Path, code: &str, name: &str, also: &[String]) -> anyhow::Result<()> {
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let also = also.iter().map(|a| q(a)).collect::<Vec<_>>().join(", ");
    let json = format!(
        "{{\n  \"format\": {FORMAT},\n  \"language\": {},\n  \"name\": {},\n  \"version\": {},\n  \"source\": \"kaikki.org (English Wiktionary)\",\n  \"license\": \"CC BY-SA 4.0\",\n  \"attribution\": \"Wiktionary contributors (CC BY-SA 4.0), via kaikki.org\",\n  \"also\": [{also}]\n}}\n",
        q(code),
        q(name),
        q(&today())
    );
    std::fs::write(dir.join("pack.json"), json)?;
    Ok(())
}

/// Write the pack; with `index`, the pre-built fst instead of forms.tsv.
fn write_pack(pack: &Pack, dir: &Path, index: bool) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut lemmas = BufWriter::new(File::create(dir.join("lemmas.tsv"))?);
    for l in &pack.lines {
        write!(lemmas, "{}\t{}\t{}", l.key, l.pos, l.gloss)?;
        if !l.written.is_empty() {
            write!(lemmas, "\t{}", l.written)?;
        }
        writeln!(lemmas)?;
    }
    lemmas.flush()?;
    let remove = |name: &str| match std::fs::remove_file(dir.join(name)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    };
    if index {
        let mut fst = fst::MapBuilder::new(BufWriter::new(File::create(dir.join("forms.fst"))?))?;
        let mut ambiguous = BufWriter::new(File::create(dir.join("ambiguous.tsv"))?);
        let mut many = 0u64;
        for (form, numbers) in &pack.forms {
            let value = if numbers.len() > 1 {
                writeln!(ambiguous, "{}", numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(","))?;
                many += 1;
                AMBIGUOUS | (many - 1)
            } else {
                numbers[0] as u64
            };
            fst.insert(form, value)?;
        }
        fst.finish()?;
        ambiguous.flush()?;
        remove("forms.tsv")?;
    } else {
        let mut forms = BufWriter::new(File::create(dir.join("forms.tsv"))?);
        for (form, numbers) in &pack.forms {
            let keys: Vec<&str> = numbers.iter().map(|&n| pack.lines[n as usize].key.as_str()).collect();
            writeln!(forms, "{form}\t{}", keys.join(","))?;
        }
        forms.flush()?;
        remove("forms.fst")?;
        remove("ambiguous.tsv")?;
    }
    Ok(())
}

/// What came out, for a look before the pack is used.
fn summary(pack: &Pack, code: &str) -> String {
    let s = &pack.stats;
    let ambiguous = pack.forms.values().filter(|n| n.len() > 1).count();
    let empty = pack.lines.iter().filter(|l| l.gloss.trim().is_empty()).count();
    let wikitext = pack.lines.iter().filter(|l| l.gloss.contains("[[") || l.gloss.contains("{{")).count();
    let mut out = format!(
        "{code}: {} lemmas, {} forms, {ambiguous} forms of more than one lemma\n  {} entries read ({} all-caps entries, {} abbreviation and {} letter-name senses skipped{})\n  links: {} from inflection tables, {} from \"form of\" senses, {} from glosses saying \"form of\", {} from alternative spellings, {} into little words dropped\n  {empty} lemmas without a gloss, {wikitext} with wikitext left in it\n",
        pack.lines.len(),
        pack.forms.len(),
        s.entries,
        s.all_caps,
        s.abbreviations,
        s.letters,
        if s.parse_errors > 0 { format!(", {} lines not parsed, first {}", s.parse_errors, s.first_error) } else { String::new() },
        s.table_links,
        s.stated_links,
        s.gloss_links,
        s.alt_links,
        s.dropped_from_little,
    );
    let mut little: Vec<&Line> = pack.lines.iter().filter(|l| l.little).collect();
    little.sort_by(|a, b| b.senses.cmp(&a.senses).then_with(|| a.key.cmp(&b.key)));
    out.push_str("The little words with most senses, as a check of the glosses:\n");
    for l in little.iter().take(25) {
        out.push_str(&format!("  {}\t{}\t{}\n", l.key, l.pos, l.gloss));
    }
    out
}

fn open(path: &Path) -> anyhow::Result<Box<dyn BufRead>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader: Box<dyn Read> = if path.extension().is_some_and(|e| e == "gz") {
        Box::new(flate2::read::MultiGzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(file)
    };
    Ok(Box::new(BufReader::with_capacity(1 << 20, reader)))
}

const USAGE: &str = "usage: langpack <code> \"<Name>\" <kaikki.jsonl.gz> <out dir> [--index]\n\
    e.g.   langpack sw Kiswahili kaikki.org-dictionary-Swahili.jsonl.gz sw";

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let index = args.iter().any(|a| a == "--index");
    args.retain(|a| a != "--index");
    let [code, name, src, out] = args.as_slice() else { bail!("{USAGE}") };
    if code.is_empty() || code.len() > 3 || !code.chars().all(|c| c.is_ascii_lowercase()) {
        bail!("the language code must be two or three lower-case letters, like sw: {USAGE}");
    }
    if name.trim().is_empty() {
        bail!("the language's name is missing: {USAGE}");
    }
    let src = PathBuf::from(src);
    let out = PathBuf::from(out);
    let pack = build(open(&src)?).with_context(|| format!("reading {}", src.display()))?;
    if pack.lines.is_empty() {
        bail!("{} gave no lemmas; is it a kaikki.org dictionary?", src.display());
    }
    if !pack.lang_code.is_empty() && pack.lang_code != *code {
        eprintln!("note: the dump says its language is \"{}\" ({}), the pack is written as {code}", pack.lang, pack.lang_code);
    }
    write_pack(&pack, &out, index).with_context(|| format!("writing {}", out.display()))?;
    write_meta(&out, code, name, &aliases(code, name, &pack.lang))?;
    print!("{}", summary(&pack, code));
    println!("written to {}{}", out.display(), if index { " with forms.fst and ambiguous.tsv" } else { "" });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-written entries in kaikki's shape, one per rule.
    const FIXTURE: &str = r#"
{"word":"ME","pos":"noun","lang":"Spanish","lang_code":"es","senses":[{"glosses":["initialism of muerte encefálica (“brain death”)"],"tags":["abbreviation","alt-of","feminine","initialism"],"alt_of":[{"word":"muerte encefálica","extra":"brain death"}]}]}
{"word":"me","pos":"pron","senses":[{"glosses":["accusative of yo: me"],"tags":["accusative","form-of"],"form_of":[{"word":"yo","extra":"me"}]},{"glosses":["dative of yo: to me, for me"],"tags":["dative","form-of"],"form_of":[{"word":"yo","extra":"to me, for me"}]},{"glosses":["reflexive of yo: myself"],"tags":["form-of","reflexive"],"form_of":[{"word":"yo","extra":"myself"}]},{"glosses":["Used as an emphatic pronoun or as an expletive."],"tags":["Latin-America","colloquial"]}]}
{"word":"yo","pos":"pron","senses":[{"glosses":["first-person singular pronoun in the nominative case; I"]}]}
{"word":"sema","pos":"verb","senses":[{"glosses":["to say, speak"]}],"forms":[{"form":"-sema","tags":["canonical"]},{"form":"kusema","tags":["infinitive"]},{"form":"positive subject concord + -lisema","tags":["past"]},{"form":"sw-conj","tags":["inflection-template"]}]}
{"word":"-enda","pos":"verb","senses":[{"glosses":["to go"]}],"forms":[{"form":"kwenda","tags":["infinitive"]}]}
{"word":"zuri","pos":"adj","senses":[{"glosses":["good, kind"],"tags":["declinable"]},{"glosses":["nice"]}],"forms":[{"form":"-zuri","tags":["canonical"]},{"form":"wazuri","tags":["error-unrecognized-form"]}]}
{"word":"mzuri","pos":"adj","senses":[{"glosses":["[[Appendix:Swahili_noun_classes#M-wa class|m class_((I))]]/[[Appendix:Swahili_noun_classes#U class|u class_((XI))]] inflected form of -zuri"]}]}
{"word":"oui","pos":"adv","senses":[{"glosses":["yes"]}]}
{"word":"oui","pos":"intj","senses":[{"glosses":["yes"]}]}
{"word":"près","pos":"adv","senses":[{"glosses":["near (a time or a place); close (to a time or a place)"]}]}
{"word":"près","pos":"prep","senses":[{"glosses":["attached to, connects a person or an organisation delegated officially to a setting."],"tags":["formal"],"topics":["law"]}]}
{"word":"a","pos":"character","senses":[{"glosses":["The first letter of the French alphabet, written in the Latin script."],"tags":["letter","lowercase"]}]}
{"word":"a","pos":"noun","senses":[{"glosses":["a, the name of the Latin-script letter A"]}],"forms":[{"form":"as","tags":["plural"]}]}
{"word":"a","pos":"pron","senses":[{"glosses":["alternative form of elle (“she”)"],"tags":["Quebec","alt-of","alternative","colloquial"],"alt_of":[{"word":"elle","extra":"she"}]}]}
{"word":"a","pos":"verb","senses":[{"glosses":["third-person singular present indicative of avoir"],"tags":["form-of"],"form_of":[{"word":"avoir"}]}]}
{"word":"avoir","pos":"verb","senses":[{"glosses":["to have"]},{"glosses":["to own"]}],"forms":[{"form":"avait","tags":["imperfect"]}]}
{"word":"elle","pos":"pron","senses":[{"glosses":["she"]}]}
{"word":"no","pos":"adv","senses":[{"glosses":["not"]}]}
{"word":"no","pos":"intj","senses":[{"glosses":["no"]}]}
{"word":"no","pos":"noun","senses":[{"glosses":["abbreviation of número; no."],"tags":["abbreviation","alt-of"],"alt_of":[{"word":"número"}]}],"forms":[{"form":"nos","tags":["plural"]}]}
{"word":"nos","pos":"pron","senses":[{"glosses":["dative of nosotros: to us, for us"],"tags":["form-of"],"form_of":[{"word":"nosotros","extra":"to us, for us"}]},{"glosses":["accusative of nosotros: us"],"tags":["form-of"],"form_of":[{"word":"nosotros","extra":"us"}]},{"glosses":["reflexive of nosotros: ourselves; each other"],"tags":["form-of"],"form_of":[{"word":"nosotros","extra":"ourselves; each other"}]}]}
{"word":"nos","pos":"noun","senses":[{"glosses":["plural of no"],"tags":["form-of","plural"],"form_of":[{"word":"no"}]}]}
{"word":"nosotros","pos":"pron","senses":[{"glosses":["we"]}]}
{"word":"Haus","pos":"noun","senses":[{"glosses":["house, building"]},{"glosses":["home (in various phrases)"]}],"forms":[{"form":"Hauses","tags":["genitive"]},{"form":"Häuser","tags":["plural"]}]}
{"word":"haus","pos":"verb","senses":[{"glosses":["singular imperative of hausen"],"tags":["form-of"],"form_of":[{"word":"hausen"}]}]}
{"word":"hausen","pos":"verb","senses":[{"glosses":["to live, to dwell"]}]}
{"word":"gehen","pos":"verb","senses":[{"glosses":["to go, to walk"]},{"glosses":["to leave"]},{"glosses":["to leave, to take off (aeroplane, train)"],"tags":["colloquial"]}],"forms":[{"form":"ging","tags":["past"]},{"form":"geht","tags":["present"]},{"form":"sein","tags":["auxiliary"]}]}
{"word":"sein","pos":"verb","senses":[{"glosses":["forms the present perfect and past perfect tenses of certain verbs"],"tags":["auxiliary"]},{"glosses":["As a copulative verb:","to be"]}],"forms":[{"form":"ist","tags":["present"]},{"form":"war","tags":["past"]}]}
{"word":"sein","pos":"det","senses":[{"glosses":["his"]},{"glosses":["its (agreeing with a masculine or neuter noun)"]}],"forms":[{"form":"seine","tags":["feminine"]}]}
{"word":"sein","pos":"pron","senses":[{"glosses":["genitive of er"],"tags":["archaic","form-of"],"form_of":[{"word":"er"}]}]}
{"word":"er","pos":"pron","senses":[{"glosses":["he"]}]}
{"word":"fue","pos":"verb","senses":[{"glosses":["third-person singular preterite indicative of ser"],"tags":["form-of"],"form_of":[{"word":"ser"}]}]}
{"word":"fue","pos":"verb","senses":[{"glosses":["third-person singular preterite indicative of ir"],"tags":["form-of"],"form_of":[{"word":"ir"}]}]}
{"word":"ser","pos":"verb","senses":[{"glosses":["to be"]}]}
{"word":"ir","pos":"verb","senses":[{"glosses":["to go"]}]}
{"word":"para","pos":"prep","senses":[{"glosses":["for, to (expressing a recipient)"]},{"glosses":["to, in order to, so, for (expressing the intended purpose of an action)"]},{"glosses":["by, due, due on, due by (expressing a deadline)"]}]}
{"word":"para","pos":"verb","senses":[{"glosses":["inflection of parar:","third-person singular present indicative"],"tags":["form-of"],"form_of":[{"word":"parar"}]}]}
{"word":"para","pos":"noun","senses":[{"glosses":["paramilitary"],"tags":["Colombia","derogatory"]}],"forms":[{"form":"paras","tags":["plural"]}]}
{"word":"parar","pos":"verb","senses":[{"glosses":["to stop"]}]}
{"word":"fangen","pos":"verb","senses":[{"glosses":["to catch"]},{"glosses":["to catch; to capture"]}]}
{"word":"der","pos":"article","senses":[{"glosses":["nominative masculine singular definite article, the"]}],"forms":[{"form":"die","tags":["feminine"]},{"form":"des","tags":["genitive"]}]}
{"word":"die","pos":"article","senses":[{"glosses":["nominative/accusative singular feminine of der"],"tags":["form-of"],"form_of":[{"word":"der"}]}]}
{"word":"die","pos":"pron","senses":[{"glosses":["inflection of der:","nominative/accusative singular feminine"],"tags":["form-of"],"form_of":[{"word":"der"}]},{"glosses":["inflection of der:","that; which; who; whom; whose"],"tags":["form-of","relative"],"form_of":[{"word":"der"}]}]}
{"word":"a.m.","pos":"adv","senses":[{"glosses":["in the morning"]}]}
{"word":"los","pos":"article","senses":[{"glosses":["masculine plural of el (“the”)"],"tags":["form-of"],"form_of":[{"word":"el","extra":"the"}]}]}
{"word":"el","pos":"article","senses":[{"glosses":["masculine singular definite article; the"]}],"forms":[{"form":"los","tags":["plural"]}]}
{"word":"estre","pos":"verb","senses":[{"glosses":["obsolete spelling of être"],"tags":["alt-of","obsolete"],"alt_of":[{"word":"être"}]}]}
{"word":"être","pos":"verb","senses":[{"glosses":["to be"]}],"forms":[{"form":"était","tags":["imperfect"]}]}
{"word":"pas","pos":"adv","senses":[{"glosses":["The most common adverb of negation in French, typically translating into English as not, don't, doesn't, etc"]}]}
{"word":"pas","pos":"noun","senses":[{"glosses":["step"]}]}
{"word":"hombre","pos":"intj","senses":[{"glosses":["man!; hey!"]}]}
{"word":"hombre","pos":"noun","senses":[{"glosses":["man"]}],"forms":[{"form":"hombres","tags":["plural"]}]}
{"word":"le","pos":"pron","senses":[{"glosses":["to him, for him; dative of él"]},{"glosses":["to her, for her; dative of ella"]}]}
{"word":"medio","pos":"adv","senses":[{"glosses":["half, incompletely"]}]}
{"word":"medio","pos":"adj","senses":[{"glosses":["half"]},{"glosses":["middle"]}]}
{"word":"medio","pos":"noun","senses":[{"glosses":["means, way"]}]}
{"word":"nous","pos":"pron","senses":[{"glosses":["the plural personal pronoun in the first person:","we"]},{"glosses":["the plural personal pronoun in the first person:","us, to us"]}]}
{"word":"nous","pos":"noun","senses":[{"glosses":["the nous, (divine) reason in philosophy"]}]}
{"word":"nach","pos":"adv","senses":[{"glosses":["Separated form of nach-"]}]}
{"word":"nach","pos":"prep","senses":[{"glosses":["after, past"]},{"glosses":["to, towards"]}]}
"#;

    fn pack() -> Pack {
        build(BufReader::new(FIXTURE.as_bytes())).unwrap()
    }

    fn line<'a>(p: &'a Pack, key: &str) -> Option<&'a Line> {
        p.lines.iter().find(|l| l.key == key)
    }

    fn lemmas_of(p: &Pack, form: &str) -> Vec<String> {
        p.forms.get(form).map(|ns| ns.iter().map(|&n| p.lines[n as usize].key.clone()).collect()).unwrap_or_default()
    }

    #[test]
    fn wikitext_and_parentheses_go() {
        assert_eq!(strip_wikitext("[[Appendix:Swahili_noun_classes#M-wa class|m class_((I))]] inflected form of -zuri"), "m class inflected form of -zuri");
        assert_eq!(strip_wikitext("see {{l|en|that}} and [[this]] [with dative]"), "see  and this ");
        assert_eq!(strip_wikitext("data; [[mobile data]"), "data; mobile data");
        assert_eq!(strip_wikitext("to [[mark up; to hit"), "to mark up; to hit");
        assert_eq!(short("to be going; to be all right (idiomatically) [with dative ‘for someone’]"), "to be going; to be all right [with dative ‘for someone’]");
        assert_eq!(short("home (in various (nested) phrases)."), "home");
        assert_eq!(short("near (a time); close (to a place)"), "near; close");
    }

    #[test]
    fn keys_lose_their_hyphens_and_odd_words_are_no_keys() {
        assert_eq!(key_of("-sema").as_deref(), Some("sema"));
        assert_eq!(key_of("Haus").as_deref(), Some("haus"));
        assert_eq!(key_of("peut-être").as_deref(), Some("peut-être"));
        assert_eq!(key_of("a.m."), None);
        assert_eq!(key_of("2cv"), None);
        assert_eq!(key_of("kommt an"), None);
        assert_eq!(key_of("¿no"), None);
        assert_eq!(target_key("-ake,").as_deref(), Some("ake"));
        assert_eq!(target_key("humu.").as_deref(), Some("humu"));
    }

    #[test]
    fn form_of_in_a_gloss_needs_the_phrase_up_front() {
        assert_eq!(form_of_in_gloss("m class/wa class inflected form of -ake").as_deref(), Some("ake"));
        assert_eq!(form_of_in_gloss("Ku class form of amba-, where").as_deref(), Some("amba"));
        assert_eq!(form_of_in_gloss("vi class inflected form and adverbial form of -le").as_deref(), Some("le"));
        assert_eq!(form_of_in_gloss("inflection of haber").as_deref(), Some("haber"));
        assert_eq!(form_of_in_gloss("a form of sunscald on hops"), None);
        assert_eq!(form_of_in_gloss("female form of Gatte"), None);
        assert_eq!(form_of_in_gloss("well"), None);
    }

    #[test]
    fn alternative_spellings_without_the_tag_are_found_by_their_gloss() {
        let s = Sense::default();
        assert_eq!(alt_targets(&s, "Former spelling of Zentaur which is no longer standard"), ["zentaur"]);
        assert_eq!(alt_targets(&s, "dated spelling of platzieren"), ["platzieren"]);
        assert!(alt_targets(&s, "tzompantli; alternative spelling of zompantli").is_empty());
        assert!(alt_targets(&s, "the form of a line").is_empty());
    }

    #[test]
    fn a_description_before_a_colon_gives_way_to_the_meaning() {
        assert_eq!(after_description("A reflexive or reciprocal pronoun: oneself, himself, herself").as_deref(), Some("oneself, himself, herself"));
        assert_eq!(after_description("neuter definite article used only before nominalized adjectives: the, that which is").as_deref(), Some("the, that which is"));
        assert_eq!(after_description("to say: to speak"), None);
        assert_eq!(after_description("the plural personal pronoun in the first person:"), None);
        let letter = Sense { glosses: vec!["he; the Hebrew letter ה".into()], ..Default::default() };
        assert!(is_letter(&letter, &letter.gloss()));
        let word = Sense { glosses: vec!["a letter to the editor".into()], ..Default::default() };
        assert!(!is_letter(&word, &word.gloss()));
    }

    #[test]
    fn grammar_only_glosses_are_no_meaning() {
        assert!(is_grammar_only("nominative/accusative feminine singular"));
        assert!(is_grammar_only("genitive plural"));
        assert!(!is_grammar_only("that; which; who; whom; whose"));
        assert!(!is_grammar_only("the"));
    }

    #[test]
    fn atoms_split_and_render_back() {
        let a = atoms("to go, to walk; to leave");
        assert_eq!(a.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["to go", "to walk", "to leave"]);
        assert_eq!(render(&a), "to go, to walk; to leave");
        assert_eq!(atoms("1,000 pieces").len(), 1);
        assert_eq!(render(&atoms("grey / gray; and/or")), "grey, gray; and/or");
    }

    #[test]
    fn long_lines_are_cut_at_atoms_then_words() {
        let mut parts = vec![atoms("one; two; three; four; five; six; seven; eight; nine; ten; eleven; twelve; thirteen; fourteen; fifteen; sixteen; seventeen; eighteen; nineteen; twenty"), atoms("a / b")];
        fit(&mut parts);
        let gloss = parts.iter().map(|p| render(p)).collect::<Vec<_>>().join(" / ");
        assert!(gloss.chars().count() <= MAX_GLOSS, "{gloss}");
        assert!(gloss.ends_with(" / a, b"), "{gloss}");
        let long = "word ".repeat(40).trim().to_string();
        let mut parts = vec![atoms(&long)];
        fit(&mut parts);
        let gloss = render(&parts[0]);
        assert!(gloss.chars().count() <= MAX_GLOSS && gloss.ends_with("word…"), "{gloss}");
    }

    #[test]
    fn initialisms_are_skipped_and_a_little_word_keeps_its_own_meaning() {
        let p = pack();
        let me = line(&p, "me").unwrap();
        assert_eq!((me.pos.as_str(), me.gloss.as_str(), me.written.as_str()), ("pron", "me; to me, for me; myself", ""));
        assert_eq!(lemmas_of(&p, "me"), ["me"]);
        assert!(!p.lines.iter().any(|l| l.gloss.contains("initialism")));
        assert!(line(&p, "a.m.").is_none());
    }

    #[test]
    fn stems_lose_their_hyphen_and_glossed_forms_link() {
        let p = pack();
        assert!(line(&p, "sema").is_some() && line(&p, "enda").is_some());
        assert_eq!(lemmas_of(&p, "kusema"), ["sema"]);
        assert_eq!(lemmas_of(&p, "kwenda"), ["enda"]);
        assert!(!p.forms.keys().any(|f| f.contains(' ') || f.starts_with('-')));
        assert_eq!(lemmas_of(&p, "mzuri"), ["zuri"]);
        assert_eq!(lemmas_of(&p, "wazuri"), ["zuri"]);
        assert!(line(&p, "mzuri").is_none());
        assert!(!p.lines.iter().any(|l| l.gloss.contains("[[") || l.gloss.contains("((")));
    }

    #[test]
    fn glosses_are_said_once_and_little_parts_lead() {
        let p = pack();
        let oui = line(&p, "oui").unwrap();
        assert_eq!((oui.pos.as_str(), oui.gloss.as_str()), ("adv,intj", "yes"));
        let pres = line(&p, "près").unwrap();
        assert_eq!((pres.pos.as_str(), pres.gloss.as_str()), ("adv,prep", "near; close"));
        assert_eq!(line(&p, "fangen").unwrap().gloss, "to catch; to capture");
        assert_eq!(line(&p, "gehen").unwrap().gloss, "to go, to walk; to leave; to take off");
        let para = line(&p, "para").unwrap();
        assert_eq!((para.pos.as_str(), para.gloss.as_str()), ("prep,noun", "for, to; in order to, so; by, due, due on, due by"));
        let no = line(&p, "no").unwrap();
        assert_eq!((no.pos.as_str(), no.gloss.as_str()), ("adv,intj", "not / no"));
        let pas = line(&p, "pas").unwrap();
        assert_eq!((pas.pos.as_str(), pas.gloss.as_str()), ("adv,noun", "The most common adverb of negation in French / step"), "cut at the comma after 30 characters; a noun may follow an adverb");
    }

    #[test]
    fn a_verb_leads_a_determiner_and_long_senses_give_way() {
        let p = pack();
        let sein = line(&p, "sein").unwrap();
        assert_eq!(sein.pos, "verb,det");
        assert_eq!(sein.gloss, "to be; forms the present perfect and past perfect tenses of certain verbs / his; its");
        assert_eq!(lemmas_of(&p, "sein"), ["sein"], "the archaic genitive of er is no link");
        assert_eq!(lemmas_of(&p, "seine"), ["sein"]);
    }

    #[test]
    fn letter_names_and_spoken_spellings_make_no_lemma_or_link() {
        let p = pack();
        assert!(line(&p, "a").is_none());
        assert_eq!(lemmas_of(&p, "a"), ["avoir"]);
        assert_eq!(lemmas_of(&p, "avait"), ["avoir"]);
        assert_eq!(lemmas_of(&p, "estre"), ["être"], "an obsolete spelling is read in old books");
        assert_eq!(lemmas_of(&p, "était"), ["être"]);
    }

    #[test]
    fn a_little_word_is_not_the_plural_of_a_noun() {
        let p = pack();
        assert_eq!(lemmas_of(&p, "nos"), ["nos"]);
        assert_eq!(line(&p, "nos").unwrap().gloss, "to us, for us; us; ourselves; each other");
        assert_eq!(lemmas_of(&p, "los"), ["los"], "the table of el does not swallow the article los");
        assert_eq!(line(&p, "los").unwrap().gloss, "the");
        assert_eq!(lemmas_of(&p, "para"), ["para", "parar"], "a verb saying para is its form does add to it");
        assert_eq!(lemmas_of(&p, "paras"), ["para"]);
        assert_eq!(lemmas_of(&p, "die"), ["der", "die"]);
        assert_eq!(line(&p, "die").unwrap().gloss, "that; which; who; whom; whose");
        assert_eq!(lemmas_of(&p, "des"), ["der"]);
    }

    #[test]
    fn german_nouns_keep_their_capital_unless_written_lower_case_somewhere() {
        let p = pack();
        let haus = line(&p, "haus").unwrap();
        assert_eq!((haus.gloss.as_str(), haus.written.as_str()), ("house, building; home", "Haus"));
        assert_eq!(lemmas_of(&p, "häuser"), ["haus"]);
        assert_eq!(lemmas_of(&p, "haus"), ["haus", "hausen"]);
        assert_eq!(line(&p, "gehen").unwrap().written, "");
        assert_eq!(lemmas_of(&p, "ging"), ["gehen"]);
        assert_eq!(lemmas_of(&p, "fue"), ["ir", "ser"]);
    }

    #[test]
    fn interjections_trail_and_only_closed_classes_keep_the_line() {
        let p = pack();
        let hombre = line(&p, "hombre").unwrap();
        assert_eq!((hombre.pos.as_str(), hombre.gloss.as_str()), ("noun,intj", "man / man!; hey!"));
        assert_eq!(lemmas_of(&p, "hombres"), ["hombre"]);
        let medio = line(&p, "medio").unwrap();
        assert_eq!((medio.pos.as_str(), medio.gloss.as_str()), ("adv,adj,noun", "half, incompletely / middle"));
        let nous = line(&p, "nous").unwrap();
        assert_eq!((nous.pos.as_str(), nous.gloss.as_str()), ("pron,noun", "we; us, to us"));
        assert_eq!(line(&p, "le").unwrap().gloss, "to him, for him; to her, for her");
        let nach = line(&p, "nach").unwrap();
        assert_eq!((nach.pos.as_str(), nach.gloss.as_str()), ("prep", "after, past; to, towards"));
        assert!(is_inflection_note("dative of él"));
        assert!(!is_inflection_note("all of us"));
        assert!(!is_inflection_note("A form of address to a woman"));
    }

    #[test]
    fn lines_are_in_byte_order() {
        let p = pack();
        let keys: Vec<&[u8]> = p.lines.iter().map(|l| l.key.as_bytes()).collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_pack_is_written_as_text_or_with_its_index() {
        let p = pack();
        let dir = std::env::temp_dir().join(format!("langpack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_pack(&p, &dir, false).unwrap();
        write_meta(&dir, "xx", "Testish", &aliases("xx", "Testish", &p.lang)).unwrap();
        let forms = std::fs::read_to_string(dir.join("forms.tsv")).unwrap();
        assert!(forms.lines().any(|l| l == "fue\tir,ser"), "{forms}");
        assert!(forms.lines().any(|l| l == "häuser\thaus"));
        assert!(!dir.join("forms.fst").exists());
        let lemmas = std::fs::read_to_string(dir.join("lemmas.tsv")).unwrap();
        assert!(lemmas.lines().any(|l| l == "haus\tnoun\thouse, building; home\tHaus"), "{lemmas}");
        assert!(lemmas.lines().any(|l| l == "me\tpron\tme; to me, for me; myself"));
        let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("pack.json")).unwrap()).unwrap();
        assert_eq!(meta["language"], "xx");
        assert_eq!(meta["format"], 1);
        assert_eq!(meta["also"], serde_json::json!(["spanish", "testish"]));
        assert_eq!(meta["version"].as_str().unwrap().len(), 10);

        write_pack(&p, &dir, true).unwrap();
        assert!(!dir.join("forms.tsv").exists());
        let fst = fst::Map::new(std::fs::read(dir.join("forms.fst")).unwrap()).unwrap();
        let ambiguous: Vec<Vec<u32>> = std::fs::read_to_string(dir.join("ambiguous.tsv")).unwrap().lines().map(|l| l.split(',').map(|n| n.parse().unwrap()).collect()).collect();
        let n = |key: &str| p.lines.iter().position(|l| l.key == key).unwrap() as u32;
        assert_eq!(fst.get("ging"), Some(n("gehen") as u64));
        let fue = fst.get("fue").unwrap();
        assert!(fue & AMBIGUOUS != 0);
        assert_eq!(ambiguous[(fue & !AMBIGUOUS) as usize], vec![n("ir"), n("ser")]);
        assert_eq!(fst.len(), p.forms.len());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_input_is_refused() {
        assert!(build(BufReader::new("not json\nnor this\n".as_bytes())).is_err());
        assert!(build(BufReader::new("".as_bytes())).is_err());
        let summary = summary(&pack(), "xx");
        assert!(summary.contains("0 lemmas without a gloss") && summary.contains("nos\tpron"));
    }
}
