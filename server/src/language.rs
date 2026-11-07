//! A book's language as one code from a fixed list. EPUBs name theirs in
//! many ways ("swe", "sv-SE", "Svenska", "UND"); the catalog keeps the
//! two-letter code (ISO 639-1) where there is one, so that books in the
//! same language are found together, and the web offers the same list. A
//! language pack in the data directory (langpack.rs) adds its own code and
//! the other names it gives for the language.

use crate::AppState;

/// The languages a book can be catalogued in: ISO 639-1 codes, plus the
/// three-letter codes and names that EPUBs use for them.
pub const LANGUAGES: &[(&str, &[&str])] = &[
    ("ar", &["ara", "arabic"]),
    ("bg", &["bul", "bulgarian"]),
    ("ca", &["cat", "catalan"]),
    ("cs", &["ces", "cze", "czech"]),
    ("cy", &["cym", "wel", "welsh"]),
    ("da", &["dan", "danish", "dansk"]),
    ("de", &["deu", "ger", "german", "deutsch"]),
    ("el", &["ell", "gre", "greek"]),
    ("en", &["eng", "english", "engelska"]),
    ("eo", &["epo", "esperanto"]),
    ("es", &["spa", "spanish", "español", "espanol"]),
    ("et", &["est", "estonian"]),
    ("eu", &["eus", "baq", "basque"]),
    ("fa", &["fas", "per", "persian"]),
    ("fi", &["fin", "finnish", "suomi"]),
    ("fo", &["fao", "faroese"]),
    ("fr", &["fra", "fre", "french", "français", "francais"]),
    ("ga", &["gle", "irish"]),
    ("he", &["heb", "hebrew"]),
    ("hi", &["hin", "hindi"]),
    ("hr", &["hrv", "croatian"]),
    ("hu", &["hun", "hungarian"]),
    ("id", &["ind", "indonesian"]),
    ("is", &["isl", "ice", "icelandic"]),
    ("it", &["ita", "italian", "italiano"]),
    ("ja", &["jpn", "japanese"]),
    ("ko", &["kor", "korean"]),
    ("la", &["lat", "latin"]),
    ("lt", &["lit", "lithuanian"]),
    ("lv", &["lav", "latvian"]),
    ("nb", &["nob", "nor", "no", "norwegian", "norsk", "bokmål", "bokmal"]),
    ("nl", &["nld", "dut", "dutch", "nederlands"]),
    ("nn", &["nno", "nynorsk"]),
    ("pl", &["pol", "polish", "polski"]),
    ("pt", &["por", "portuguese", "português", "portugues"]),
    ("ro", &["ron", "rum", "romanian"]),
    ("ru", &["rus", "russian"]),
    ("se", &["sme", "northern sami"]),
    ("sk", &["slk", "slo", "slovak"]),
    ("sl", &["slv", "slovenian"]),
    ("sr", &["srp", "serbian"]),
    ("sv", &["swe", "swedish", "svenska"]),
    ("sw", &["swa", "swahili", "kiswahili"]),
    ("ta", &["tam", "tamil"]),
    ("tr", &["tur", "turkish"]),
    ("uk", &["ukr", "ukrainian"]),
    ("yi", &["yid", "yiddish"]),
    ("zh", &["zho", "chi", "chinese"]),
];

/// Whether a string is shaped like a language code here: two or three
/// lower-case ASCII letters (ISO 639-1, or 639-3 for a language without a
/// two-letter code).
pub fn is_code(s: &str) -> bool {
    (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase())
}

/// The catalog's code for a language as an EPUB or a person wrote it: from
/// the list, else from the installed packs' codes and other names (`packs`:
/// (name, code) pairs, as `Packs::aliases` gives them); None when it names
/// no language known here ("und", "mul", empty).
pub fn normalize_with(written: &str, packs: &[(String, String)]) -> Option<String> {
    let lower = written.trim().to_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("").trim();
    if primary.is_empty() {
        return None;
    }
    if let Some((code, _)) = LANGUAGES.iter().find(|(code, also)| *code == primary || also.contains(&primary) || also.contains(&lower.as_str())) {
        return Some(code.to_string());
    }
    packs.iter().find(|(name, _)| name == primary || *name == lower).map(|(_, code)| code.clone())
}

/// `normalize_with` for the list alone.
#[cfg(test)]
pub fn normalize(written: &str) -> Option<String> {
    normalize_with(written, &[])
}

/// What to store for a language given: the list's or a pack's code, or the
/// value as it was when it is not known (an operator's own choice is kept).
pub fn catalog_value_with(written: Option<&str>, packs: &[(String, String)]) -> Option<String> {
    let written = written.map(str::trim).filter(|w| !w.is_empty())?;
    match written.to_lowercase().as_str() {
        "und" | "mul" | "zxx" => None,
        _ => Some(normalize_with(written, packs).unwrap_or_else(|| written.to_string())),
    }
}

/// `catalog_value_with` for the list alone; the sweep below brings what a
/// pack's names would have caught to the pack's code later.
pub fn catalog_value(written: Option<&str>) -> Option<String> {
    catalog_value_with(written, &[])
}

/// Bring the languages already in the catalog to the list's codes, once at
/// start and then now and then (a pack installed since adds its names).
/// Values not known are left as they are.
pub async fn normalize_catalog(state: &AppState) -> anyhow::Result<u64> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, language FROM books WHERE language IS NOT NULL").fetch_all(&state.db).await?;
    let packs = state.packs.aliases();
    let mut changed = 0;
    for (id, written) in rows {
        let value = catalog_value_with(Some(&written), &packs);
        if value.as_deref() != Some(written.as_str()) {
            sqlx::query("UPDATE books SET language = $1 WHERE id = $2").bind(value).bind(id).execute(&state.db).await?;
            changed += 1;
        }
    }
    if changed > 0 {
        tracing::info!("languages: {changed} books given the list's language code");
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ways_epubs_name_a_language_become_one_code() {
        for written in ["sv", "swe", "sv-SE", "SV_se", "Swedish", "svenska"] {
            assert_eq!(normalize(written).as_deref(), Some("sv"), "{written}");
        }
        assert_eq!(normalize("en-GB").as_deref(), Some("en"));
        assert_eq!(normalize("EN-US").as_deref(), Some("en"));
        assert_eq!(normalize("eng").as_deref(), Some("en"));
        assert_eq!(normalize("epo").as_deref(), Some("eo"));
        assert_eq!(normalize("no").as_deref(), Some("nb"));
        assert_eq!(normalize("nn-NO").as_deref(), Some("nn"));
        assert_eq!(normalize("swa").as_deref(), Some("sw"));
        assert_eq!(normalize("Kiswahili").as_deref(), Some("sw"));
        assert_eq!(normalize("klingon"), None);
    }

    #[test]
    fn an_installed_packs_names_reach_its_code() {
        let packs = vec![("yor".to_string(), "yor".to_string()), ("yoruba".to_string(), "yor".to_string()), ("èdè yorùbá".to_string(), "yor".to_string())];
        assert_eq!(normalize_with("Yoruba", &packs).as_deref(), Some("yor"));
        assert_eq!(normalize_with("yor-NG", &packs).as_deref(), Some("yor"));
        assert_eq!(normalize_with("Èdè Yorùbá", &packs).as_deref(), Some("yor"));
        assert_eq!(normalize_with("Yoruba", &[]), None);
        assert_eq!(catalog_value_with(Some("yoruba"), &packs).as_deref(), Some("yor"));
        assert_eq!(normalize_with("swe", &packs).as_deref(), Some("sv"), "the list first");
    }

    #[test]
    fn no_language_is_stored_as_none_and_unknown_ones_are_kept() {
        assert_eq!(catalog_value(Some("UND")), None);
        assert_eq!(catalog_value(Some("  ")), None);
        assert_eq!(catalog_value(None), None);
        assert_eq!(catalog_value(Some("sv-SE")).as_deref(), Some("sv"));
        assert_eq!(catalog_value(Some("tlh")).as_deref(), Some("tlh"));
    }

    #[test]
    fn a_code_is_two_or_three_small_letters() {
        assert!(is_code("sw") && is_code("yor"));
        assert!(!is_code("s") && !is_code("Sw") && !is_code("sw-KE") && !is_code("swah") && !is_code(""));
    }
}

#[cfg(test)]
mod web_list {
    /// The web's picker offers the same list as the server keeps.
    #[test]
    fn the_web_offers_the_same_languages() {
        let web = include_str!("../../web/src/lib/library/languages.ts");
        for (code, _) in super::LANGUAGES {
            assert!(web.contains(&format!("'{code}'")), "{code} is missing from the web's list");
        }
        assert_eq!(web.matches("\t'").count(), super::LANGUAGES.len());
    }
}
