//! Authors as people, out of the free-text author field, as the web reader
//! has them (web/src/lib/library/authors.ts): "Selma Lagerlöf", "Lagerlöf,
//! Selma" and "Lagerlöf, Selma, 1858-1940" are one person, and "Maj Sjöwall,
//! Per Wahlöö" and "Sjöwall & Wahlöö" are two. Kept in step with that file.

use crate::authorfacts::key_of;

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    /// "Selma Lagerlöf"
    pub name: String,
    /// "Lagerlöf Selma", for ordering by surname.
    pub sort: String,
    /// Lowercase, no diacritics, no punctuation: the identity.
    pub key: String,
}

/// Words that belong to the surname: "Le Guin", "van Gogh", "af Klint", "de la Motte".
const PARTICLES: &[&str] = &[
    "le", "la", "de", "del", "della", "der", "den", "des", "di", "da", "du", "van", "von", "af", "av", "ten", "ter", "zu", "dos", "das",
    "st.", "mac", "o",
];

fn particle(word: &str) -> bool {
    PARTICLES.contains(&word.to_lowercase().as_str())
}

fn person(first: &str, surname: &str) -> Person {
    let squeeze = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = squeeze(format!("{first} {surname}"));
    let sort = squeeze(format!("{surname} {first}"));
    let key = key_of(&name);
    Person { name, sort, key }
}

/// Dates and roles that Libris and others append: "1858-1940", "f. 1946", "(red.)".
fn is_noise(segment: &str) -> bool {
    let s = segment.trim();
    if s.is_empty() || (s.starts_with('(') && s.ends_with(')')) {
        return true;
    }
    let digits = s.chars().fold((0, 0), |(run, best), c| if c.is_ascii_digit() { (run + 1, best.max(run + 1)) } else { (0, best) }).1;
    if digits >= 3 {
        return true;
    }
    let role = s.strip_suffix('.').unwrap_or(s).to_lowercase();
    ["red", "ed", "eds", "övers", "transl", "ill", "förf", "author", "editor"].contains(&role.as_str())
}

/// One name in natural order: the last word is the surname, with any particles before it.
fn natural(raw: &str) -> Option<Person> {
    let mut words: Vec<&str> = raw.split_whitespace().collect();
    let mut surname = vec![words.pop()?];
    while words.len() > 1 && particle(words[words.len() - 1]) {
        surname.insert(0, words.pop()?);
    }
    Some(person(&words.join(" "), &surname.join(" ")))
}

fn from_commas(part: &str) -> Vec<Person> {
    let segments: Vec<&str> = part.split(',').map(str::trim).filter(|s| !is_noise(s)).collect();
    match segments.len() {
        0 => return Vec::new(),
        1 => return natural(segments[0]).into_iter().collect(),
        _ => {}
    }
    let one_word = |s: &str| !s.contains(char::is_whitespace);
    // A surname may carry particles: "Le Guin", "van Eyck".
    let surname_like = |s: &str| {
        let words: Vec<&str> = s.split_whitespace().collect();
        words[..words.len().saturating_sub(1)].iter().all(|w| particle(w))
    };
    // "Lagerlöf, Selma" (and "Le Guin, Ursula K."): surname first.
    if segments.len() == 2 && surname_like(segments[0]) {
        return vec![person(segments[1], segments[0])];
    }
    // "Strindberg, August, Lagerlöf, Selma": surname/first pairs.
    if segments.len() % 2 == 0 && segments.iter().all(|s| one_word(s)) {
        return segments.chunks(2).map(|pair| person(pair[1], pair[0])).collect();
    }
    // "Maj Sjöwall, Per Wahlöö": several names in natural order.
    segments.iter().filter_map(|s| natural(s)).collect()
}

/// Where one field holds several people: ";", "&", and "and" in a few languages.
fn parts(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in raw.split([';', '&']) {
        let mut current: Vec<&str> = Vec::new();
        for word in piece.split_whitespace() {
            if ["och", "and", "und", "et", "y"].contains(&word.to_lowercase().as_str()) && !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            } else {
                current.push(word);
            }
        }
        out.push(current.join(" "));
    }
    out
}

/// The people in an author field, in the order written, without duplicates.
pub fn split(author: Option<&str>) -> Vec<Person> {
    let Some(raw) = author.map(str::trim).filter(|a| !a.is_empty()) else {
        return Vec::new();
    };
    let mut people: Vec<Person> = Vec::new();
    for p in parts(raw).iter().flat_map(|part| from_commas(part)) {
        if !p.key.is_empty() && !people.iter().any(|q| q.key == p.key) {
            people.push(p);
        }
    }
    people
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(author: &str) -> Vec<String> {
        split(Some(author)).into_iter().map(|p| p.name).collect()
    }

    #[test]
    fn one_person_written_every_way() {
        for written in ["Selma Lagerlöf", "Lagerlöf, Selma", "Lagerlöf, Selma, 1858-1940", "Lagerlöf, Selma, (red.)"] {
            let people = split(Some(written));
            assert_eq!(people.len(), 1, "{written}");
            assert_eq!(people[0].name, "Selma Lagerlöf");
            assert_eq!(people[0].sort, "Lagerlöf Selma");
            assert_eq!(people[0].key, "selma lagerlof");
        }
    }

    #[test]
    fn several_people_in_one_field() {
        assert_eq!(names("Maj Sjöwall, Per Wahlöö"), ["Maj Sjöwall", "Per Wahlöö"]);
        assert_eq!(names("Sjöwall & Wahlöö"), ["Sjöwall", "Wahlöö"]);
        assert_eq!(names("Strindberg, August, Lagerlöf, Selma"), ["August Strindberg", "Selma Lagerlöf"]);
        assert_eq!(names("Terry Pratchett and Neil Gaiman"), ["Terry Pratchett", "Neil Gaiman"]);
        assert_eq!(names("Maj Sjöwall; Per Wahlöö; Maj Sjöwall"), ["Maj Sjöwall", "Per Wahlöö"]);
    }

    #[test]
    fn particles_stay_with_the_surname() {
        let p = &split(Some("Ursula K. Le Guin"))[0];
        assert_eq!((p.name.as_str(), p.sort.as_str()), ("Ursula K. Le Guin", "Le Guin Ursula K."));
        assert_eq!(split(Some("Le Guin, Ursula K."))[0].key, p.key);
        assert_eq!(split(Some("Hilma af Klint"))[0].sort, "af Klint Hilma");
    }

    #[test]
    fn nothing_for_nothing() {
        assert!(split(None).is_empty());
        assert!(split(Some("  ")).is_empty());
        assert!(split(Some("1858-1940")).is_empty());
    }
}
