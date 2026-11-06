//! ESPDIC, Paul Denisowski's Esperanto–English dictionary (CC BY 3.0), built
//! into the server: a meaning for nearly every Esperanto word without asking
//! anyone. A word not in it as it stands is tried as the dictionary has it
//! (ending, then one affix off), since Esperanto builds its words from roots.

use crate::dictionary::esperanto_forms;
use std::collections::HashMap;
use std::sync::OnceLock;

const ESPDIC: &str = include_str!("../assets/espdic.txt");

/// Who to name wherever a meaning from here is shown.
pub const ATTRIBUTION: &str = "ESPDIC, Paul Denisowski (CC BY 3.0)";

/// A meaning found: the headword it was found under, the English, and whether
/// the word had to be taken apart to find it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub headword: String,
    pub english: String,
    pub derived: bool,
}

fn index() -> &'static HashMap<String, String> {
    static INDEX: OnceLock<HashMap<String, String>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<String, String> = HashMap::with_capacity(64_000);
        for line in ESPDIC.lines().filter(|l| !l.starts_with('#')) {
            let Some((eo, en)) = line.split_once(" : ") else { continue };
            let key = eo.trim().to_lowercase();
            map.entry(key)
                .and_modify(|e| {
                    e.push_str("; ");
                    e.push_str(en.trim());
                })
                .or_insert_with(|| en.trim().to_string());
        }
        map
    })
}

const SUFFIXES: [&str; 26] = [
    "et", "eg", "in", "ul", "ej", "ist", "ar", "ig", "iĝ", "ad", "aĵ", "ec", "an", "er", "il", "ism", "uj", "id", "ind", "ebl", "em",
    "estr", "ing", "obl", "on", "op",
];
const PREFIXES: [&str; 11] = ["mal", "re", "ek", "ge", "dis", "eks", "fi", "mis", "pra", "bo", "for"];

/// The root under the grammatical ending, and the ending's word class.
fn root(word: &str) -> Option<(&str, char)> {
    let bare = word.trim_end_matches('n').trim_end_matches('j');
    for (ending, class) in [("as", 'i'), ("is", 'i'), ("os", 'i'), ("us", 'i'), ("u", 'i'), ("i", 'i'), ("o", 'o'), ("a", 'a'), ("e", 'e')] {
        if let Some(root) = bare.strip_suffix(ending) {
            if root.chars().count() >= 2 {
                return Some((root, class));
            }
        }
    }
    None
}

pub fn lookup(word: &str) -> Option<Found> {
    let lower = word.trim().to_lowercase();
    let lower = crate::esperanto::supersigned(&lower).unwrap_or(lower);
    let index = index();
    for form in esperanto_forms(&lower) {
        if let Some(en) = index.get(&form) {
            return Some(Found { headword: form, english: en.clone(), derived: false });
        }
    }
    // Taken apart: one suffix or one prefix off the root, under any word class.
    let (root, class) = root(&lower)?;
    let mut tries: Vec<String> = Vec::new();
    for suffix in SUFFIXES {
        if let Some(stem) = root.strip_suffix(suffix) {
            if stem.chars().count() >= 2 {
                tries.push(stem.to_string());
            }
        }
    }
    for prefix in PREFIXES {
        if let Some(stem) = root.strip_prefix(prefix) {
            if stem.chars().count() >= 2 {
                tries.push(stem.to_string());
            }
        }
    }
    for stem in tries {
        let mut classes = vec![class];
        classes.extend(['o', 'a', 'i', 'e'].into_iter().filter(|c| *c != class));
        for c in classes {
            let headword = format!("{stem}{c}");
            if let Some(en) = index.get(&headword) {
                return Some(Found { headword, english: en.clone(), derived: true });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_found_as_the_dictionary_has_it() {
        let f = lookup("diris").unwrap();
        assert_eq!(f.headword, "diri");
        assert!(f.english.contains("say"), "{f:?}");
        assert!(!f.derived);
        assert_eq!(lookup("Kiam").unwrap().headword, "kiam");
        assert_eq!(lookup("cxambrojn").unwrap().headword, "ĉambro");
    }

    #[test]
    fn a_word_built_from_a_root_is_taken_apart() {
        let f = lookup("hundetegojn").unwrap();
        assert_eq!(f.headword, "hundeto");
        assert!(f.derived);
    }

    #[test]
    fn nothing_for_what_is_not_esperanto() {
        assert_eq!(lookup("xyzzyq"), None);
    }
}
