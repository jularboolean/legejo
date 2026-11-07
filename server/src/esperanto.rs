//! Esperanto's regular grammar, as far as searching and counting words need
//! it: the x-system for its letters, the words that take no ending, and a
//! word's root, headword and class by its endings.
//!
//! On a keyboard without ĉ, ĝ, ĥ, ĵ, ŝ and ŭ, Esperanto is typed in the
//! x-system: cx, gx, hx, jx, sx, ux. A search typed that way should find
//! the words as books print them.

use std::collections::HashMap;

/// The word with every x-system pair as the letter it stands for, or None
/// when it has none. Both cases: "Cxambro" and "CXAMBRO" become "Ĉambro"
/// and "ĈAMBRO".
pub fn supersigned(word: &str) -> Option<String> {
    let mut out = String::with_capacity(word.len());
    let mut chars = word.chars().peekable();
    let mut changed = false;
    while let Some(c) = chars.next() {
        let letter = match c {
            'c' => Some('ĉ'),
            'g' => Some('ĝ'),
            'h' => Some('ĥ'),
            'j' => Some('ĵ'),
            's' => Some('ŝ'),
            'u' => Some('ŭ'),
            'C' => Some('Ĉ'),
            'G' => Some('Ĝ'),
            'H' => Some('Ĥ'),
            'J' => Some('Ĵ'),
            'S' => Some('Ŝ'),
            'U' => Some('Ŭ'),
            _ => None,
        };
        match (letter, chars.peek()) {
            (Some(letter), Some('x' | 'X')) => {
                chars.next();
                out.push(letter);
                changed = true;
            }
            _ => out.push(c),
        }
    }
    changed.then_some(out)
}

// ---------------------------------------------------------------- grammar

pub const PRONOUNS: [&str; 10] = ["mi", "vi", "li", "ŝi", "ĝi", "ni", "ili", "si", "oni", "ci"];
/// Words that take no grammatical ending and must not lose a letter that
/// looks like one ("pri", "tre", "tro").
pub const FIXED: &[&str] = &[
    "la", "kaj", "de", "en", "al", "ne", "ke", "sed", "por", "kun", "el", "pri", "sur", "sub", "tra", "ĉe", "ĝis", "post", "antaŭ",
    "ĉar", "se", "ankaŭ", "nur", "jam", "tre", "tro", "pli", "plej", "ol", "aŭ", "ĉu", "eĉ", "ja", "do", "tamen", "nun", "hodiaŭ",
    "morgaŭ", "hieraŭ", "ankoraŭ", "baldaŭ", "apenaŭ", "preskaŭ", "ĉirkaŭ", "kontraŭ", "laŭ", "malgraŭ", "krom", "inter", "per",
    "pro", "sen", "super", "apud", "ekster", "je", "po", "ĉi", "for", "ju", "des", "mem", "nek", "plu", "tuj", "ambaŭ", "jen",
    "jes", "ho", "ve", "unu", "du", "tri", "kvar", "kvin", "ses", "sep", "ok", "naŭ", "dek", "cent", "mil", "adiaŭ", "almenaŭ",
    "kvazaŭ", "ĉiam", "neniam", "ajn", "eble", "dum", "kiam", "tiam", "iam",
];

const CORRELATIVE_STARTS: [&str; 5] = ["neni", "ĉi", "ki", "ti", "i"];
const CORRELATIVE_ENDS: [&str; 9] = ["u", "o", "a", "e", "am", "al", "el", "om", "es"];

/// Whether the word is one of the table words ("kiu", "tiam", "nenio").
pub fn is_correlative(word: &str) -> bool {
    for prefix in CORRELATIVE_STARTS {
        if let Some(rest) = word.strip_prefix(prefix) {
            if CORRELATIVE_ENDS.contains(&rest) {
                return true;
            }
        }
    }
    false
}

/// The little words: those without endings, the pronouns and the
/// correlatives.
pub fn little_words() -> Vec<String> {
    let mut out: Vec<String> = FIXED.iter().chain(PRONOUNS.iter()).map(|w| w.to_string()).collect();
    for prefix in CORRELATIVE_STARTS {
        for end in CORRELATIVE_ENDS {
            out.push(format!("{prefix}{end}"));
        }
    }
    out
}

/// A word's root: without the plural and accusative endings and the ending
/// of its word class. Pronouns keep their possessives with them ("mia" is
/// "mi"), correlatives and endless words stay whole.
pub fn stem(word: &str) -> String {
    // Words without endings first: the j of "kaj" and "plej" is no plural.
    if FIXED.contains(&word) {
        return word.to_string();
    }
    let mut bare = word;
    if bare.chars().count() > 2 {
        bare = bare.strip_suffix('n').unwrap_or(bare);
    }
    if bare.chars().count() > 2 {
        bare = bare.strip_suffix('j').unwrap_or(bare);
    }
    if FIXED.contains(&bare) || is_correlative(bare) {
        return bare.to_string();
    }
    if PRONOUNS.contains(&bare) {
        return bare.to_string();
    }
    if let Some(pronoun) = bare.strip_suffix('a').filter(|p| PRONOUNS.contains(p)) {
        return pronoun.to_string();
    }
    for ending in ["as", "is", "os", "us", "u", "i", "o", "a", "e"] {
        if let Some(root) = bare.strip_suffix(ending) {
            if root.chars().count() >= 2 {
                // A word built on a correlative or an endless word ("kialo",
                // a reason, on "kial", why; "unua" on "unu") is a word of its
                // own, not a form of the little word: it keeps its ending.
                if is_correlative(root) || FIXED.contains(&root) {
                    return bare.to_string();
                }
                return root.to_string();
            }
        }
    }
    bare.to_string()
}

/// The dictionary form of a stem from the forms seen: verbs as -i, nouns
/// -o, adjectives -a, adverbs -e; a word without ending as it is.
pub fn headword(stem: &str, forms: &[(String, u32)]) -> String {
    let Some((top, _)) = forms.first() else { return stem.to_string() };
    // Correlatives, pronouns and words without endings are their own headword ("ĉiu", not "ĉiuj").
    if FIXED.contains(&stem) || is_correlative(stem) || PRONOUNS.contains(&stem) {
        return stem.to_string();
    }
    if top == stem {
        return top.clone();
    }
    let mut classes: HashMap<char, u32> = HashMap::new();
    for (form, n) in forms {
        let Some(rest) = form.strip_prefix(stem) else { continue };
        let rest = rest.trim_end_matches('n').trim_end_matches('j');
        let class = match rest {
            "as" | "is" | "os" | "us" | "u" | "i" => 'i',
            "o" => 'o',
            "a" => 'a',
            "e" => 'e',
            _ => continue,
        };
        *classes.entry(class).or_default() += n;
    }
    match classes.into_iter().max_by_key(|(c, n)| (*n, *c)) {
        Some((class, _)) => format!("{stem}{class}"),
        None => top.clone(),
    }
}

/// A headword's class, by its ending: "verb", "noun", "adj" or "other".
pub fn class(word: &str) -> &'static str {
    if FIXED.contains(&word) || PRONOUNS.contains(&word) || is_correlative(word) || word.chars().count() < 3 {
        return "other";
    }
    match word.chars().last() {
        Some('i') => "verb",
        Some('o') => "noun",
        Some('a') => "adj",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_x_system_becomes_the_letters_it_stands_for() {
        assert_eq!(supersigned("cxambro").as_deref(), Some("ĉambro"));
        assert_eq!(supersigned("sxangxigxi").as_deref(), Some("ŝanĝiĝi"));
        assert_eq!(supersigned("auxto").as_deref(), Some("aŭto"));
        assert_eq!(supersigned("hxoro").as_deref(), Some("ĥoro"));
        assert_eq!(supersigned("jxurnalo").as_deref(), Some("ĵurnalo"));
        assert_eq!(supersigned("Cxambro").as_deref(), Some("Ĉambro"));
        assert_eq!(supersigned("CXAMBRO").as_deref(), Some("ĈAMBRO"));
        assert_eq!(supersigned("ĉambro"), None);
        assert_eq!(supersigned("Dracula"), None);
        assert_eq!(supersigned("xenon"), None, "an x that follows no such letter stays");
        assert_eq!(supersigned("tax"), None);
    }

    #[test]
    fn words_come_to_their_roots() {
        for (word, root) in [
            ("diris", "dir"),
            ("diri", "dir"),
            ("domojn", "dom"),
            ("bona", "bon"),
            ("bone", "bon"),
            ("mia", "mi"),
            ("min", "mi"),
            ("ilin", "ili"),
            ("kiu", "kiu"),
            ("kiujn", "kiu"),
            ("pri", "pri"),
            ("tre", "tre"),
            ("tro", "tro"),
            ("la", "la"),
            ("kaj", "kaj"),
            ("plej", "plej"),
            ("ajn", "ajn"),
            ("kial", "kial"),
            ("kialo", "kialo"),
            ("kialon", "kialo"),
            ("kialoj", "kialo"),
            ("tiama", "tiama"),
            ("unua", "unua"),
        ] {
            assert_eq!(stem(word), root, "{word}");
        }
    }

    #[test]
    fn a_word_built_on_a_correlative_is_not_the_correlative() {
        assert_eq!(headword("kialo", &[("kialo".into(), 3), ("kialon".into(), 1)]), "kialo");
        assert_eq!(headword("kial", &[("kial".into(), 3)]), "kial");
        assert_eq!(class("kialo"), "noun");
        assert_eq!(class("kial"), "other");
        assert!(is_correlative("kial") && is_correlative("neniu") && !is_correlative("kialo") && !is_correlative("kia-"));
        let fixed: std::collections::HashSet<&str> = FIXED.iter().copied().collect();
        assert_eq!(fixed.len(), FIXED.len(), "no word twice");
    }
}
