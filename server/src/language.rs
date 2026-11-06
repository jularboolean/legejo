//! A book's language as one code from a fixed list. EPUBs name theirs in
//! many ways ("swe", "sv-SE", "Svenska", "UND"); the catalog keeps the
//! two-letter code (ISO 639-1) where there is one, so that books in the
//! same language are found together, and the web offers the same list.

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
    ("ta", &["tam", "tamil"]),
    ("tr", &["tur", "turkish"]),
    ("uk", &["ukr", "ukrainian"]),
    ("yi", &["yid", "yiddish"]),
    ("zh", &["zho", "chi", "chinese"]),
];

/// The catalog's code for a language as an EPUB or a person wrote it, or
/// None when it names no language on the list ("und", "mul", empty).
pub fn normalize(written: &str) -> Option<&'static str> {
    let lower = written.trim().to_lowercase();
    let primary = lower.split(['-', '_']).next().unwrap_or("").trim();
    if primary.is_empty() {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|(code, also)| *code == primary || also.contains(&primary) || also.contains(&lower.as_str()))
        .map(|(code, _)| *code)
}

/// What to store for a language given: the list's code, or the value as
/// it was when it is not on the list (an operator's own choice is kept).
pub fn catalog_value(written: Option<&str>) -> Option<String> {
    let written = written.map(str::trim).filter(|w| !w.is_empty())?;
    match written.to_lowercase().as_str() {
        "und" | "mul" | "zxx" => None,
        _ => Some(normalize(written).map(str::to_string).unwrap_or_else(|| written.to_string())),
    }
}

/// Bring the languages already in the catalog to the list's codes, once at
/// start. Values not on the list are left as they are.
pub async fn normalize_catalog(state: &AppState) -> anyhow::Result<u64> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, language FROM books WHERE language IS NOT NULL").fetch_all(&state.db).await?;
    let mut changed = 0;
    for (id, written) in rows {
        let value = catalog_value(Some(&written));
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
            assert_eq!(normalize(written), Some("sv"), "{written}");
        }
        assert_eq!(normalize("en-GB"), Some("en"));
        assert_eq!(normalize("EN-US"), Some("en"));
        assert_eq!(normalize("eng"), Some("en"));
        assert_eq!(normalize("epo"), Some("eo"));
        assert_eq!(normalize("no"), Some("nb"));
        assert_eq!(normalize("nn-NO"), Some("nn"));
        assert_eq!(normalize("klingon"), None);
    }

    #[test]
    fn no_language_is_stored_as_none_and_unknown_ones_are_kept() {
        assert_eq!(catalog_value(Some("UND")), None);
        assert_eq!(catalog_value(Some("  ")), None);
        assert_eq!(catalog_value(None), None);
        assert_eq!(catalog_value(Some("sv-SE")).as_deref(), Some("sv"));
        assert_eq!(catalog_value(Some("tlh")).as_deref(), Some("tlh"));
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
