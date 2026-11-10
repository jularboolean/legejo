//! How far a learner's vocabulary reaches into a book: the share of its
//! words among the 1 000, 2 000 and 5 000 commonest of the language, how
//! many of the commonest it takes to know 95 % of the text (what reading
//! without a dictionary at hand is held to need), and the CEFR level at
//! which a learner knows about that many. Needs a frequency list, which a
//! language pack's extras may have (langpack.rs).

use super::text::Prepared;
use crate::langpack::Pack;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The share of the text a reader needs to know.
const READABLE: f64 = 0.95;
const BANDS: [u32; 3] = [1000, 2000, 5000];
/// The words a learner knows at the end of each level, about: the upper
/// ends of the ranges vocabulary-size studies give for A1 to C1.
const LEVELS: [(u32, &str); 5] = [(1000, "A1"), (2000, "A2"), (3250, "B1"), (5000, "B2"), (8000, "C1")];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reach {
    /// (the commonest so many lemmas, the share of the text they are).
    pub within: Vec<(u32, f64)>,
    /// How many of the commonest lemmas make up 95 % of the text; none
    /// when the list is too short to say.
    pub need: Option<u32>,
    /// "A1" to "C2"; none when the text is beyond a list too short to
    /// tell C1 from C2.
    pub cefr: Option<String>,
    /// Where the frequency list comes from.
    pub source: Option<String>,
}

/// The level at which a learner knows `need` words. A text that a list of
/// `ranked` lemmas does not reach is C2 only if the list goes past C1.
fn cefr(need: Option<u32>, ranked: usize) -> Option<&'static str> {
    match need {
        Some(n) => Some(LEVELS.iter().find(|(most, _)| n <= *most).map(|(_, level)| *level).unwrap_or("C2")),
        None => (ranked as u32 >= LEVELS[LEVELS.len() - 1].0).then_some("C2"),
    }
}

/// Names are left out of both sides, as in the study list: a reader knows
/// them without learning them. A word that can be several lemmas counts as
/// the commonest of them.
pub(super) fn reach(p: &Prepared, pack: &Pack, names: &HashSet<u32>) -> Option<Reach> {
    let extras = pack.extras().filter(|e| e.has_frequency())?;
    let counts = p.counts();
    let mut ranked: Vec<(u32, u64)> = Vec::new();
    let mut total = 0u64;
    for id in 0..p.lower.len() as u32 {
        if names.contains(&id) {
            continue;
        }
        let n = counts[id as usize] as u64;
        total += n;
        if let Some(rank) = pack.lemmas_of(p.word(id)).into_iter().filter_map(|l| extras.rank(l)).min() {
            ranked.push((rank, n));
        }
    }
    if total == 0 {
        return None;
    }
    ranked.sort_unstable();
    let within = BANDS
        .iter()
        .map(|band| {
            let known: u64 = ranked.iter().take_while(|(rank, _)| rank <= band).map(|(_, n)| n).sum();
            (*band, (known as f64 / total as f64 * 1000.0).round() / 1000.0)
        })
        .collect();
    let mut known = 0u64;
    let need = ranked.iter().find_map(|(rank, n)| {
        known += n;
        (known as f64 >= total as f64 * READABLE).then_some(*rank)
    });
    Some(Reach { within, need, cefr: cefr(need, extras.ranked()).map(String::from), source: extras.meta.frequency.clone() })
}

#[cfg(test)]
mod tests {
    use super::super::text::prepare;
    use super::*;

    #[test]
    fn the_level_follows_the_words_it_takes() {
        for (need, level) in [(800, "A1"), (2000, "A2"), (3000, "B1"), (4000, "B2"), (7000, "C1"), (12000, "C2")] {
            assert_eq!(cefr(Some(need), 20_000), Some(level));
        }
        assert_eq!(cefr(None, 20_000), Some("C2"));
        assert_eq!(cefr(None, 3000), None, "a short list cannot say how far beyond it the text is");
    }

    #[test]
    fn a_frequency_list_says_how_far_the_commonest_words_reach() {
        let dir = std::env::temp_dir().join(format!("legejo-reach-{}", crate::books::new_uuid()));
        let xx = dir.join("languages/xx");
        crate::langpack::write_xx(&xx, "to run", "1");
        let packs = crate::langpack::Packs::new(dir.clone());
        let text = "Ko blorped zam flib ko zams. ".repeat(100);
        let p = prepare(&text);
        assert!(reach(&p, &packs.get("xx").unwrap(), &HashSet::new()).is_none(), "no list, nothing said");

        // The list: ko 1, blorp 2, zam 3; "flib" is not on it.
        crate::langpack::write_xx_extras(&xx);
        let pack = packs.reload("xx").unwrap();
        let r = reach(&p, &pack, &HashSet::new()).unwrap();
        // flib can be blorp too, the commoner of its lemmas: every word is
        // within the three commonest.
        assert_eq!(r.within, [(1000, 1.0), (2000, 1.0), (5000, 1.0)]);
        assert_eq!(r.need, Some(3));
        assert_eq!(r.cefr.as_deref(), Some("A1"));
        assert_eq!(r.source.as_deref(), Some("A word list, CC0"));
        // A text of words the list does not have is beyond it.
        let text = "Ko glorf snarf plim ko trask. ".repeat(100);
        let unknown = prepare(&text);
        let r = reach(&unknown, &pack, &HashSet::new()).unwrap();
        assert_eq!(r.within[0], (1000, 0.333));
        assert_eq!((r.need, r.cefr), (None, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
