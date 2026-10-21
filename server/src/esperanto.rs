//! Esperanto written without its own letters. On a keyboard without ĉ, ĝ, ĥ,
//! ĵ, ŝ and ŭ, Esperanto is typed in the x-system: cx, gx, hx, jx, sx, ux.
//! A search typed that way should find the words as books print them.

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
}
