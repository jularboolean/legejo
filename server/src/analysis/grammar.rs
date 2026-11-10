//! A book's grammar: its words counted by the forms they stand in (tense,
//! participle, case, number), with the commonest forms of each, and the
//! table of a word's forms. Esperanto's endings say what a form is; for
//! another language its pack's extras do (langpack.rs: tags.tsv), where a
//! form that can be several things is counted as possibly each.

use super::text::Prepared;
use super::{bad_request, not_found};
use crate::esperanto::{self, FIXED, PRONOUNS};
use crate::langpack::{Extras, Pack};
use crate::AppState;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Forms given as examples of each kind.
const EXAMPLES: usize = 4;

/// One kind of form: how many of the book's words stand in it, and the
/// commonest of them.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Counted {
    pub tag: String,
    pub count: u64,
    /// Words that may stand in it: their form can be something else too.
    #[serde(default)]
    pub possible: u64,
    pub examples: Vec<String>,
}

/// The kinds of one sort ("verb": present, past, future …) and the words
/// they are counted among.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Group {
    pub key: String,
    pub of: u64,
    pub items: Vec<Counted>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Grammar {
    /// How the forms were told apart: "endings" or "pack".
    pub source: String,
    pub groups: Vec<Group>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Class {
    Verb,
    Noun,
    Adjective,
    Adverb,
}

impl Class {
    fn tag(self) -> &'static str {
        match self {
            Class::Verb => "verb",
            Class::Noun => "noun",
            Class::Adjective => "adjective",
            Class::Adverb => "adverb",
        }
    }
}

/// What an Esperanto word's endings say it is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shape {
    class: Class,
    /// A verb's form: "present", "past", "future", "conditional",
    /// "imperative" or "infinitive".
    verb: Option<&'static str>,
    /// "active-present" … "passive-future".
    participle: Option<&'static str>,
    plural: bool,
    accusative: bool,
}

const VERB_ENDINGS: [(&str, &str); 6] =
    [("as", "present"), ("is", "past"), ("os", "future"), ("us", "conditional"), ("u", "imperative"), ("i", "infinitive")];
const PARTICIPLES: [(&str, &str); 6] = [
    ("ant", "active-present"),
    ("int", "active-past"),
    ("ont", "active-future"),
    ("at", "passive-present"),
    ("it", "passive-past"),
    ("ot", "passive-future"),
];
/// Words without endings that FIXED does not list and that end like a form.
const ENDLESS: [&str; 6] = ["plus", "minus", "ekde", "disde", "ĵus", "adiaŭ"];

/// A lower-case Esperanto word's shape; none for a little word.
fn shape(word: &str) -> Option<Shape> {
    if word.chars().count() < 3 || FIXED.contains(&word) || ENDLESS.contains(&word) || word.contains(['\'', '’']) {
        return None;
    }
    let accusative = word.ends_with('n');
    let bare = word.strip_suffix('n').unwrap_or(word);
    let plural = bare.ends_with('j');
    let bare = bare.strip_suffix('j').unwrap_or(bare);
    if FIXED.contains(&bare) || esperanto::is_correlative(bare) || PRONOUNS.contains(&bare) || bare.strip_suffix('a').is_some_and(|p| PRONOUNS.contains(&p)) {
        return None;
    }
    if !accusative && !plural {
        for (ending, tag) in VERB_ENDINGS {
            if bare.strip_suffix(ending).is_some_and(|root| root.chars().count() >= 2) {
                return Some(Shape { class: Class::Verb, verb: Some(tag), participle: None, plural, accusative });
            }
        }
    }
    let (root, class) = match bare.chars().next_back()? {
        'o' => (&bare[..bare.len() - 1], Class::Noun),
        'a' => (&bare[..bare.len() - 1], Class::Adjective),
        // "hejmen", homewards: an adverb with the ending of direction.
        'e' if !plural => (&bare[..bare.len() - 1], Class::Adverb),
        _ => return None,
    };
    if root.chars().count() < 2 {
        return None;
    }
    Some(Shape { class, verb: None, participle: participle(root), plural, accusative })
}

/// Roots that look like a participle of a verb there is and are not one:
/// "spirito" is not what is breathed, "volonte" not about to want.
const NO_PARTICIPLES: [&str; 4] = ["spirit", "kredit", "limit", "volont"];

/// The participle a root ends in ("far-it"), when the dictionary has the
/// verb it would be of: "soldato" and "elefanto" are words of their own.
fn participle(root: &str) -> Option<&'static str> {
    if NO_PARTICIPLES.contains(&root) {
        return None;
    }
    PARTICIPLES.iter().find_map(|(suffix, tag)| {
        let verb = format!("{}i", root.strip_suffix(suffix).filter(|r| r.chars().count() >= 2)?);
        crate::espdic::lookup(&verb).is_some_and(|f| !f.derived && f.headword == verb).then_some(*tag)
    })
}

/// The commonest forms among these, most common first.
fn examples(forms: HashMap<u32, u32>, p: &Prepared) -> Vec<String> {
    let mut forms: Vec<(u32, u32)> = forms.into_iter().collect();
    forms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| p.word(a.0).cmp(p.word(b.0))));
    forms.into_iter().take(EXAMPLES).map(|(id, _)| p.word(id).to_string()).collect()
}

/// The sorts of forms a pack's tags are counted by, each with its kinds.
/// Number and gender are left out: a table tags the plural and the feminine
/// but not the lemma itself, so their shares would say nothing.
const SORTS: [(&str, &[&str]); 5] = [
    ("tense", &["present", "preterite", "imperfect", "past", "future", "perfect", "pluperfect"]),
    ("mood", &["indicative", "subjunctive", "conditional", "imperative", "infinitive", "gerund", "participle"]),
    ("person", &["first-person", "second-person", "third-person"]),
    ("case", &["nominative", "accusative", "genitive", "dative"]),
    ("degree", &["comparative", "superlative"]),
];

/// The lemmas a word of the book is read as. A word that is a lemma itself
/// ("casa", "nada", "calle") is that word and no form of another, unless
/// the book uses another of its lemmas at least as much in forms that can
/// only be that one: "era" is the verb in a book full of "eran" and "soy".
/// The study list reads a word the same way (vocabulary/count.rs).
fn read_as(word: &str, count: u32, pack: &Pack, sure: &HashMap<u32, u32>) -> Vec<u32> {
    let lemmas = pack.lemmas_of(word);
    let Some(own) = lemmas.iter().copied().find(|l| pack.lemma(*l).is_some_and(|x| x.word == word)) else {
        return lemmas;
    };
    let other = lemmas
        .iter()
        .copied()
        .filter(|l| pack.lemma(*l).is_some_and(|x| x.word != word))
        .filter(|l| sure.get(l).copied().unwrap_or(0) >= count)
        .max_by_key(|l| (sure.get(l).copied().unwrap_or(0), std::cmp::Reverse(*l)));
    vec![other.unwrap_or(own)]
}

/// What a lower-case word can be, by the pack's tags, read as these lemmas:
/// per sort, its kinds, and whether the one kind is sure (every reading of
/// the word has it).
fn kinds_of(word: &str, lemmas: &[u32], pack: &Pack, extras: &Extras) -> Vec<(usize, &'static str, bool)> {
    let mut readings: Vec<Vec<&str>> = Vec::new();
    // A reading the tags say nothing of.
    let mut untold = false;
    for lemma in lemmas {
        let Some(l) = pack.lemma(*lemma) else { continue };
        let told = extras.tags_of(l.word, word);
        untold |= told.is_empty();
        readings.extend(told);
    }
    if readings.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (sort, (_, kinds)) in SORTS.iter().enumerate() {
        let found: Vec<&'static str> = kinds.iter().copied().filter(|k| readings.iter().any(|r| r.contains(k))).collect();
        let every = readings.iter().all(|r| r.iter().any(|t| kinds.contains(t)));
        let sure = found.len() == 1 && every && !untold;
        out.extend(found.into_iter().map(|kind| (sort, kind, sure)));
    }
    out
}

/// A book's forms counted by a pack's tags.
fn grammar_by_pack(p: &Prepared, pack: &Pack, names: &HashSet<u32>) -> Option<Grammar> {
    let extras = pack.extras().filter(|e| e.has_tags())?;
    let counts = p.counts();
    // How much the book uses each lemma in forms that can only be that one.
    let mut sure: HashMap<u32, u32> = HashMap::new();
    for id in 0..p.lower.len() as u32 {
        if let [one] = pack.lemmas_of(p.word(id)).as_slice() {
            *sure.entry(*one).or_default() += counts[id as usize];
        }
    }
    let kinds: Vec<Vec<(usize, &'static str, bool)>> = (0..p.lower.len() as u32)
        .map(|id| {
            if names.contains(&id) {
                return Vec::new();
            }
            let word = p.word(id);
            kinds_of(word, &read_as(word, counts[id as usize], pack, &sure), pack, extras)
        })
        .collect();
    /// A kind's sure and possible counts, and the forms seen for each.
    #[derive(Default)]
    struct Seen {
        sure: u64,
        possible: u64,
        forms: HashMap<u32, u32>,
        maybe: HashMap<u32, u32>,
    }
    let mut seen: HashMap<(usize, &str), Seen> = HashMap::new();
    let mut of = [0u64; SORTS.len()];
    for id in &p.ids {
        let mut counted = [false; SORTS.len()];
        for (sort, kind, sure) in &kinds[*id as usize] {
            if !counted[*sort] {
                counted[*sort] = true;
                of[*sort] += 1;
            }
            let entry = seen.entry((*sort, *kind)).or_default();
            if *sure {
                entry.sure += 1;
                *entry.forms.entry(*id).or_default() += 1;
            } else {
                entry.possible += 1;
                *entry.maybe.entry(*id).or_default() += 1;
            }
        }
    }
    let groups: Vec<Group> = SORTS
        .iter()
        .enumerate()
        .map(|(sort, (key, tags))| Group {
            key: key.to_string(),
            of: of[sort],
            items: tags
                .iter()
                .filter_map(|tag| {
                    let s = seen.remove(&(sort, *tag))?;
                    // The forms that are surely of the kind show it best.
                    let forms = if s.forms.is_empty() { s.maybe } else { s.forms };
                    Some(Counted { tag: tag.to_string(), count: s.sure, possible: s.possible, examples: examples(forms, p) })
                })
                .collect(),
        })
        .filter(|g| !g.items.is_empty())
        .collect();
    (!groups.is_empty()).then(|| Grammar { source: "pack".to_string(), groups })
}

pub(super) fn grammar(p: &Prepared, language: &str, pack: Option<&Pack>, names: &HashSet<u32>) -> Option<Grammar> {
    if language != "eo" {
        return grammar_by_pack(p, pack?, names);
    }
    let shapes: Vec<Option<Shape>> = (0..p.lower.len() as u32).map(|id| if names.contains(&id) { None } else { shape(p.word(id)) }).collect();
    // Per kind of form: its count and the forms it was seen in.
    let mut seen: HashMap<(&str, &str), (u64, HashMap<u32, u32>)> = HashMap::new();
    let mut of: HashMap<&str, u64> = HashMap::new();
    for id in &p.ids {
        let Some(s) = shapes[*id as usize] else { continue };
        let mut note = |group: &'static str, tag: &'static str| {
            let entry = seen.entry((group, tag)).or_default();
            entry.0 += 1;
            *entry.1.entry(*id).or_default() += 1;
        };
        note("class", s.class.tag());
        *of.entry("class").or_default() += 1;
        if let Some(tag) = s.verb {
            note("verb", tag);
            *of.entry("verb").or_default() += 1;
        }
        if let Some(tag) = s.participle {
            note("participle", tag);
            *of.entry("participle").or_default() += 1;
        }
        if matches!(s.class, Class::Noun | Class::Adjective) {
            *of.entry("noun").or_default() += 1;
            if s.plural {
                note("noun", "plural");
            }
            if s.accusative {
                note("noun", "accusative");
            }
        }
    }
    let mut group = |key: &'static str, tags: &[&'static str]| {
        let items: Vec<Counted> = tags
            .iter()
            .filter_map(|tag| seen.remove(&(key, *tag)).map(|(count, forms)| Counted { tag: tag.to_string(), count, possible: 0, examples: examples(forms, p) }))
            .collect();
        Group { key: key.to_string(), of: of.get(key).copied().unwrap_or(0), items }
    };
    let groups: Vec<Group> = vec![
        group("verb", &VERB_ENDINGS.map(|(_, tag)| tag)),
        group("participle", &PARTICIPLES.map(|(_, tag)| tag)),
        group("noun", &["accusative", "plural"]),
        group("class", &["noun", "verb", "adjective", "adverb"]),
    ]
    .into_iter()
    .filter(|g| !g.items.is_empty())
    .collect();
    (!groups.is_empty()).then(|| Grammar { source: "endings".to_string(), groups })
}

// ------------------------------------------------------- a word's forms

#[derive(Serialize, Debug, PartialEq)]
pub struct Row {
    pub tags: Vec<String>,
    pub form: String,
    /// Whether this is the form asked about.
    pub this: bool,
}

#[derive(Serialize, Debug)]
pub struct Table {
    pub key: String,
    /// What the forms of the table have in common ("present",
    /// "subjunctive"); empty when the key says it.
    pub title: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Serialize, Debug)]
pub struct Forms {
    /// The dictionary form.
    pub word: String,
    /// "verb", "noun", "adjective" or "adverb"; a pack's part of speech.
    pub class: String,
    /// The participle the word is, with the verb it is of.
    pub participle: Option<&'static str>,
    pub of: Option<String>,
    pub tables: Vec<Table>,
}

/// The table of an Esperanto word's forms: every form its class has, the
/// one asked about marked.
fn forms_eo(written: &str) -> Option<Forms> {
    let word = written.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let word = esperanto::supersigned(&word).unwrap_or(word);
    let s = shape(&word)?;
    let bare = word.strip_suffix('n').unwrap_or(&word);
    let bare = bare.strip_suffix('j').unwrap_or(bare);
    let row = |tags: Vec<&'static str>, form: String| Row { this: form == word, tags: tags.into_iter().map(String::from).collect(), form };
    let table = |key: &str, rows: Vec<Row>| Table { key: key.to_string(), title: Vec::new(), rows };
    let nominal = |root: &str, vowel: char, key: &'static str| Table {
        key: key.to_string(),
        title: Vec::new(),
        rows: vec![
            row(vec!["singular"], format!("{root}{vowel}")),
            row(vec!["plural"], format!("{root}{vowel}j")),
            row(vec!["singular", "accusative"], format!("{root}{vowel}n")),
            row(vec!["plural", "accusative"], format!("{root}{vowel}jn")),
        ],
    };
    let (root, headword, tables) = match s.class {
        Class::Verb => {
            let ending = VERB_ENDINGS.iter().find(|(e, _)| bare.ends_with(e)).map(|(e, _)| e.len()).unwrap_or(0);
            let root = &bare[..bare.len() - ending];
            let verb = table("verb", VERB_ENDINGS.iter().rev().map(|(e, tag)| row(vec![tag], format!("{root}{e}"))).collect());
            let participles = table("participle", PARTICIPLES.iter().map(|(suffix, tag)| row(vec![tag], format!("{root}{suffix}a"))).collect());
            (root, format!("{root}i"), vec![verb, participles])
        }
        Class::Noun => {
            let root = &bare[..bare.len() - 1];
            (root, format!("{root}o"), vec![nominal(root, 'o', "noun")])
        }
        Class::Adjective => {
            let root = &bare[..bare.len() - 1];
            (root, format!("{root}a"), vec![nominal(root, 'a', "adjective")])
        }
        Class::Adverb => {
            let root = &bare[..bare.len() - 1];
            let rows = vec![row(vec!["adverb"], format!("{root}e")), row(vec!["direction"], format!("{root}en"))];
            (root, format!("{root}e"), vec![table("adverb", rows)])
        }
    };
    // A participle is a form of its verb: "farita" of "fari".
    let of = s.participle.and_then(|tag| {
        let suffix = PARTICIPLES.iter().find(|(_, t)| *t == tag)?.0;
        Some(format!("{}i", root.strip_suffix(suffix)?))
    });
    Some(Forms { word: headword, class: s.class.tag().to_string(), participle: s.participle, of, tables })
}

/// The tags that say which form of a table a row is; the others say which
/// table it stands in.
const SLOT: [&str; 8] = ["first-person", "second-person", "third-person", "singular", "plural", "masculine", "feminine", "neuter"];

/// The table of a word's forms from a pack's tags: the forms of its lemma
/// laid out by what they have in common (tense and mood, case), the rows
/// by person, number and gender. Of the lemmas the word can be, the one
/// whose tables have the word; else the first with any.
fn forms_by_pack(written: &str, pack: &Pack) -> Option<Forms> {
    let extras = pack.extras().filter(|e| e.has_tags())?;
    let word = written.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let lemmas: Vec<u32> = pack.lemmas_of(&word);
    let told = |n: &u32| pack.lemma(*n).is_some_and(|l| !extras.tags_of(l.word, &word).is_empty());
    let has_forms = |n: &u32| pack.lemma(*n).is_some_and(|l| !extras.forms_of(l.word).is_empty());
    let lemma = pack.lemma(*lemmas.iter().find(|n| told(n)).or_else(|| lemmas.iter().find(|n| has_forms(n)))?)?;
    let place = |tag: &str, order: &[&str]| order.iter().position(|t| *t == tag).unwrap_or(order.len());
    let mut tables: Vec<Table> = Vec::new();
    for (form, readings) in extras.forms_of(lemma.word) {
        for reading in readings {
            // Forms with a pronoun attached and regional forms are left out.
            if reading.iter().any(|t| *t == "combined" || *t == "variant") {
                continue;
            }
            let (slot, title): (Vec<&str>, Vec<&str>) = reading.iter().partition(|t| SLOT.contains(t));
            let key = title.join("-");
            let row = Row { tags: slot.iter().map(|t| t.to_string()).collect(), this: form == word, form: form.clone() };
            match tables.iter_mut().find(|t| t.key == key) {
                Some(t) if !t.rows.contains(&row) => t.rows.push(row),
                Some(_) => {}
                None => tables.push(Table { key, title: title.iter().map(|t| t.to_string()).collect(), rows: vec![row] }),
            }
        }
    }
    // Tables by mood, then tense; rows by number, then person, then gender.
    let all: Vec<&str> = SORTS.iter().flat_map(|(_, kinds)| kinds.iter().copied()).collect();
    let sort_key = |t: &Table| {
        let of = |sort: usize| t.title.iter().map(|tag| place(tag, SORTS[sort].1)).filter(|at| *at < SORTS[sort].1.len()).min();
        // A tense without a mood named (Dutch, German) stands with the indicative.
        let mood = of(1).or(of(0).map(|_| 0)).unwrap_or(usize::MAX);
        (mood, of(0).unwrap_or(usize::MAX), t.title.iter().map(|tag| place(tag, &all)).min().unwrap_or(usize::MAX), t.key.clone())
    };
    tables.sort_by_key(sort_key);
    for t in &mut tables {
        t.rows.sort_by_key(|r| {
            let of = |from: usize, to: usize| r.tags.iter().map(|tag| place(tag, &SLOT[from..to])).min().unwrap_or(usize::MAX);
            (of(3, 5), of(0, 3), of(5, 8), r.form.clone())
        });
    }
    let class = lemma.pos.split(',').next().unwrap_or("").to_string();
    Some(Forms { word: lemma.written.to_string(), class, participle: None, of: None, tables })
}

#[derive(Deserialize)]
pub struct FormsQuery {
    language: String,
    w: String,
}

/// GET /api/analysis/forms?language=&w=: the table of a word's forms, the
/// one asked about marked. 404 for a language whose forms are not known
/// here, or a word without forms (a little word).
pub async fn forms(State(state): State<AppState>, _user: crate::auth::AuthUser, Query(q): Query<FormsQuery>) -> Response {
    if q.w.chars().count() > 60 {
        return bad_request("a word");
    }
    let found = match q.language.as_str() {
        "eo" => forms_eo(&q.w),
        language => crate::vocabulary::pack_for(&state, language).and_then(|pack| forms_by_pack(&q.w, &pack)),
    };
    match found {
        Some(forms) => Json(forms).into_response(),
        None => not_found("no forms"),
    }
}

/// Whether a language's words have tables of forms here.
pub(crate) fn has_forms(state: &AppState, language: &str) -> bool {
    language == "eo" || crate::vocabulary::pack_for(state, language).is_some_and(|p| p.extras().is_some_and(|e| e.has_tags()))
}

#[cfg(test)]
mod tests {
    use super::super::text::prepare;
    use super::*;

    #[test]
    fn the_endings_say_what_a_form_is() {
        let s = |w: &str| shape(w).unwrap_or_else(|| panic!("{w}"));
        assert_eq!(s("faras").verb, Some("present"));
        assert_eq!(s("venis").verb, Some("past"));
        assert_eq!(s("estus").verb, Some("conditional"));
        assert_eq!(s("iru").verb, Some("imperative"));
        assert_eq!(s("legi").verb, Some("infinitive"));
        let hundojn = s("hundojn");
        assert!(hundojn.class == Class::Noun && hundojn.plural && hundojn.accusative);
        assert!(s("granda").class == Class::Adjective && !s("granda").plural);
        assert_eq!(s("hejmen").class, Class::Adverb);
        assert_eq!(s("farita").participle, Some("passive-past"));
        assert_eq!(s("legantoj").participle, Some("active-present"));
        assert_eq!(s("vidinte").participle, Some("active-past"));
        // Words of their own that only look like participles.
        for word in ["soldato", "elefanto", "kanto", "monto", "spirito", "horizonto", "volonte"] {
            assert_eq!(s(word).participle, None, "{word}");
        }
        // Little words have no forms.
        for word in ["la", "kaj", "mi", "min", "mia", "miajn", "kiu", "tiujn", "ĝis", "tri", "pli", "unu", "ĵus"] {
            assert!(shape(word).is_none(), "{word}");
        }
    }

    #[test]
    fn a_books_forms_are_counted_by_kind() {
        let text = "La hundo vidis la grandajn katojn kaj kuris hejmen. Poste diris Petro ke li venos morgaŭ, kaj la hundo bojas. ".repeat(40);
        let p = prepare(&text);
        let names = super::super::phrases::name_words(&p, "eo");
        let g = grammar(&p, "eo", None, &names).unwrap();
        assert_eq!(g.source, "endings");
        let verb = g.groups.iter().find(|g| g.key == "verb").unwrap();
        assert_eq!(verb.of, 200);
        let past = verb.items.iter().find(|i| i.tag == "past").unwrap();
        assert_eq!(past.count, 120);
        assert_eq!(past.examples, ["diris", "kuris", "vidis"]);
        assert_eq!(verb.items.iter().map(|i| i.tag.as_str()).collect::<Vec<_>>(), ["present", "past", "future"]);
        let noun = g.groups.iter().find(|g| g.key == "noun").unwrap();
        // hundo twice, grandajn, katojn: Petro is a name and not counted.
        assert_eq!(noun.of, 160);
        assert_eq!(noun.items.iter().find(|i| i.tag == "accusative").unwrap().count, 80);
        assert!(g.groups.iter().all(|g| g.key != "participle"));
        assert!(grammar(&p, "es", None, &names).is_none());
    }

    #[test]
    fn a_words_forms_are_laid_out_with_the_one_asked_about_marked() {
        let f = forms_eo("Hundojn").unwrap();
        assert_eq!((f.word.as_str(), f.class.as_str()), ("hundo", "noun"));
        let rows: Vec<(&str, bool)> = f.tables[0].rows.iter().map(|r| (r.form.as_str(), r.this)).collect();
        assert_eq!(rows, [("hundo", false), ("hundoj", false), ("hundon", false), ("hundojn", true)]);
        let f = forms_eo("venis").unwrap();
        assert_eq!(f.word, "veni");
        assert_eq!(f.tables[0].rows[0], Row { tags: vec!["infinitive".to_string()], form: "veni".into(), this: false });
        assert!(f.tables[0].rows.iter().any(|r| r.form == "venis" && r.this && r.tags == ["past"]));
        assert_eq!(f.tables[1].rows.len(), 6);
        // The x-system is read, and a participle names its verb.
        let f = forms_eo("sxangxita").unwrap();
        assert_eq!((f.word.as_str(), f.participle, f.of.as_deref()), ("ŝanĝita", Some("passive-past"), Some("ŝanĝi")));
        assert!(forms_eo("kaj").is_none());
    }

    /// A pack for a made-up language with a verb in two tenses, a form that
    /// is two things, and a form that is a noun as well.
    fn tagged_pack() -> (std::path::PathBuf, crate::langpack::Packs) {
        let dir = std::env::temp_dir().join(format!("legejo-grammar-{}", crate::books::new_uuid()));
        let xx = dir.join("languages/xx");
        std::fs::create_dir_all(&xx).unwrap();
        std::fs::write(xx.join("pack.json"), r#"{"format": 1, "language": "xx", "name": "Xxish", "version": "1", "source": "", "license": "", "attribution": ""}"#).unwrap();
        std::fs::write(xx.join("lemmas.tsv"), "blorp\tverb\tto run\nflib\tnoun\tbox\nko\tprep\tto\n").unwrap();
        std::fs::write(xx.join("forms.tsv"), "blorp\tblorp\nblorps\tblorp\nblorped\tblorp\nblorpe\tblorp\nflib\tflib,blorp\nko\tko\n").unwrap();
        std::fs::write(
            xx.join("tags.tsv"),
            "blorp\tblorp\tinfinitive\nblorps\tblorp\tthird-person singular present indicative\nblorped\tblorp\tthird-person singular past indicative\n\
             blorpe\tblorp\tfirst-person singular present subjunctive;third-person singular present subjunctive\nflib\tblorp\tsecond-person singular imperative\n",
        )
        .unwrap();
        (dir.clone(), crate::langpack::Packs::new(dir))
    }

    #[test]
    fn a_packs_tags_count_the_forms_and_a_form_of_two_kinds_is_possibly_each() {
        let (dir, packs) = tagged_pack();
        let pack = packs.get("xx").unwrap();
        // Per sentence: blorps (present), blorped (past) twice, blorpe (first
        // or third person), flib (an imperative or the noun).
        let text = "Zam blorps ko flib. Ko blorped zam blorped ko. Zam blorpe ko blorp. ".repeat(100);
        let p = prepare(&text);
        let g = grammar(&p, "xx", Some(&pack), &HashSet::new()).unwrap();
        assert_eq!(g.source, "pack");
        let group = |key: &str| g.groups.iter().find(|g| g.key == key).unwrap_or_else(|| panic!("{key}: {:?}", g.groups));
        let item = |key: &str, tag: &str| group(key).items.iter().find(|i| i.tag == tag).unwrap_or_else(|| panic!("{tag}")).clone();
        assert_eq!(group("tense").of, 400);
        assert_eq!((item("tense", "present").count, item("tense", "past").count), (200, 200));
        assert_eq!(item("tense", "past").examples, ["blorped"]);
        // The subjunctive is sure, its person is not.
        assert_eq!((item("mood", "subjunctive").count, item("mood", "subjunctive").possible), (100, 0));
        assert_eq!((item("person", "first-person").count, item("person", "first-person").possible), (0, 100));
        // "flib" is a noun of its own too, but the book uses the verb far
        // more in forms that are only the verb's: it is read as the verb.
        assert_eq!((item("mood", "imperative").count, item("mood", "imperative").possible), (100, 0));
        assert_eq!(item("mood", "imperative").examples, ["flib"]);
        assert_eq!(item("mood", "infinitive").count, 100);
        assert!(g.groups.iter().all(|g| g.key != "case"));
        // In a book that hardly uses the verb otherwise, "flib" is the noun
        // and is not counted as a form of the verb.
        let text = "Zam flib ko flib. Ko flib zam flib ko. Zam blorped ko flib. ".repeat(100);
        let p = prepare(&text);
        let g = grammar(&p, "xx", Some(&pack), &HashSet::new()).unwrap();
        assert!(g.groups.iter().flat_map(|g| &g.items).all(|i| i.tag != "imperative"), "{:?}", g.groups);

        // The table of the verb, from any of its forms.
        let f = forms_by_pack("Blorped", &pack).unwrap();
        assert_eq!((f.word.as_str(), f.class.as_str()), ("blorp", "verb"));
        let titles: Vec<String> = f.tables.iter().map(|t| t.title.join(" ")).collect();
        assert_eq!(titles, ["present indicative", "past indicative", "present subjunctive", "imperative", "infinitive"]);
        let past = &f.tables[1].rows;
        assert_eq!(past.len(), 1);
        assert!(past[0].this && past[0].form == "blorped" && past[0].tags == ["third-person", "singular"]);
        let subjunctive: Vec<&str> = f.tables[2].rows.iter().map(|r| r.tags[0].as_str()).collect();
        assert_eq!(subjunctive, ["first-person", "third-person"]);
        // "flib" is the verb's form here, since the noun has no table.
        assert_eq!(forms_by_pack("flib", &pack).unwrap().word, "blorp");
        assert!(forms_by_pack("ko", &pack).is_none() && forms_by_pack("zzz", &pack).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
