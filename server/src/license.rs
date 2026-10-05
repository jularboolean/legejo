//! Book licences and the federation gate.
//!
//! A shelf may only federate books that are demonstrably free: a licence
//! from the fixed list below, a source URL for the free edition, and for
//! public domain the year the last author died. `federable` is the single
//! check, called wherever a book can leave the instance. Local features
//! (public shelves, import, OPDS, Kobo) are not gated by licence.

use serde::{Deserialize, Serialize};

/// The licences Legejo knows. Stored as the serde name ("cc-by-sa"), never
/// as free text; anything else in the column reads as unknown.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum License {
    /// Public domain in Sweden: 70 years after the end of the death year.
    Pd,
    Cc0,
    CcBy,
    CcBySa,
    CcByNc,
    CcByNcSa,
    CcByNd,
    CcByNcNd,
    /// Protected; may sit on instance shelves, never federate.
    Copyright,
}

impl License {
    pub const ALL: [License; 9] = [
        License::Pd,
        License::Cc0,
        License::CcBy,
        License::CcBySa,
        License::CcByNc,
        License::CcByNcSa,
        License::CcByNd,
        License::CcByNcNd,
        License::Copyright,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            License::Pd => "pd",
            License::Cc0 => "cc0",
            License::CcBy => "cc-by",
            License::CcBySa => "cc-by-sa",
            License::CcByNc => "cc-by-nc",
            License::CcByNcSa => "cc-by-nc-sa",
            License::CcByNd => "cc-by-nd",
            License::CcByNcNd => "cc-by-nc-nd",
            License::Copyright => "copyright",
        }
    }

    pub fn parse(s: &str) -> Option<License> {
        License::ALL.into_iter().find(|l| l.as_str() == s)
    }
}

/// The licence facts of one book, as stored.
#[derive(Clone, Debug, Default)]
pub struct LicenseFacts {
    pub license: Option<String>,
    pub source_url: Option<String>,
    pub author_death_year: Option<i64>,
}

/// Why a book may not federate. Serialized as `{"code": …, …}` so the web
/// can phrase it in the reader's language.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum NotFederable {
    UnknownLicense,
    Copyrighted,
    MissingSource,
    MissingDeathYear,
    /// Public domain claimed, but the Swedish term has not run out yet.
    StillProtected { died: i64, free_from: i64 },
}

/// The first year a work is free in Sweden: protection lasts until the end
/// of the 70th year after the (last) author's death.
pub fn free_from(death_year: i64) -> i64 {
    death_year + 71
}

/// The gate: Ok when the book may leave the instance in `year`.
pub fn federable(facts: &LicenseFacts, year: i64) -> Result<(), NotFederable> {
    let license = facts
        .license
        .as_deref()
        .and_then(License::parse)
        .ok_or(NotFederable::UnknownLicense)?;
    if license == License::Copyright {
        return Err(NotFederable::Copyrighted);
    }
    if facts.source_url.as_deref().map_or(true, |u| valid_source_url(u).is_err()) {
        return Err(NotFederable::MissingSource);
    }
    if license == License::Pd {
        let died = facts.author_death_year.ok_or(NotFederable::MissingDeathYear)?;
        if year < free_from(died) {
            return Err(NotFederable::StillProtected { died, free_from: free_from(died) });
        }
    }
    Ok(())
}

pub fn current_year() -> i64 {
    time::OffsetDateTime::now_utc().year() as i64
}

/// A source must be an absolute http(s) URL with a host.
pub fn valid_source_url(url: &str) -> Result<(), &'static str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("the source must be an http(s) address")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || url.chars().any(char::is_whitespace) || url.len() > 2000 {
        return Err("the source must be an http(s) address");
    }
    Ok(())
}

/// Death years outside this range are typing mistakes.
pub fn valid_death_year(year: i64) -> Result<(), &'static str> {
    if (-3000..=current_year()).contains(&year) {
        Ok(())
    } else {
        Err("the year of death is not plausible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(license: &str, source: Option<&str>, died: Option<i64>) -> LicenseFacts {
        LicenseFacts {
            license: Some(license.into()),
            source_url: source.map(Into::into),
            author_death_year: died,
        }
    }

    const SRC: Option<&str> = Some("https://runeberg.org/rodarummet/");

    #[test]
    fn names_round_trip_through_serde_and_parse() {
        for l in License::ALL {
            let json = serde_json::to_string(&l).unwrap();
            assert_eq!(json, format!("\"{}\"", l.as_str()));
            assert_eq!(License::parse(l.as_str()), Some(l));
        }
        assert_eq!(License::parse("CC-BY"), None);
        assert_eq!(License::parse("gpl"), None);
    }

    #[test]
    fn public_domain_runs_seventy_years_after_the_death_year() {
        // Strindberg died 1912: free long since.
        assert_eq!(federable(&facts("pd", SRC, Some(1912)), 2026), Ok(()));
        // Died 1955: protected through 2025, free from 2026.
        assert_eq!(federable(&facts("pd", SRC, Some(1955)), 2026), Ok(()));
        assert_eq!(
            federable(&facts("pd", SRC, Some(1955)), 2025),
            Err(NotFederable::StillProtected { died: 1955, free_from: 2026 })
        );
        // Died 1960: free from 2031.
        assert_eq!(
            federable(&facts("pd", SRC, Some(1960)), 2026),
            Err(NotFederable::StillProtected { died: 1960, free_from: 2031 })
        );
        assert_eq!(free_from(1961), 2032);
        assert_eq!(federable(&facts("pd", SRC, None), 2026), Err(NotFederable::MissingDeathYear));
    }

    #[test]
    fn every_free_licence_needs_a_source() {
        for l in License::ALL.into_iter().filter(|l| !matches!(l, License::Copyright | License::Pd)) {
            assert_eq!(federable(&facts(l.as_str(), SRC, None), 2026), Ok(()), "{l:?}");
            assert_eq!(federable(&facts(l.as_str(), None, None), 2026), Err(NotFederable::MissingSource));
            assert_eq!(
                federable(&facts(l.as_str(), Some("runeberg.org"), None), 2026),
                Err(NotFederable::MissingSource)
            );
        }
    }

    #[test]
    fn unknown_and_protected_never_pass() {
        assert_eq!(federable(&LicenseFacts::default(), 2026), Err(NotFederable::UnknownLicense));
        assert_eq!(federable(&facts("gpl", SRC, None), 2026), Err(NotFederable::UnknownLicense));
        assert_eq!(federable(&facts("copyright", SRC, Some(1800)), 2026), Err(NotFederable::Copyrighted));
    }

    #[test]
    fn reasons_serialize_with_a_code() {
        let v = serde_json::to_value(NotFederable::StillProtected { died: 1961, free_from: 2032 }).unwrap();
        assert_eq!(v, serde_json::json!({ "code": "still_protected", "died": 1961, "free_from": 2032 }));
        let v = serde_json::to_value(NotFederable::MissingSource).unwrap();
        assert_eq!(v, serde_json::json!({ "code": "missing_source" }));
    }

    #[test]
    fn source_urls() {
        assert!(valid_source_url("https://www.gutenberg.org/ebooks/1").is_ok());
        assert!(valid_source_url("http://runeberg.org/x/").is_ok());
        assert!(valid_source_url("ftp://x.org").is_err());
        assert!(valid_source_url("https://").is_err());
        assert!(valid_source_url("https:///path").is_err());
        assert!(valid_source_url("https://a b.org").is_err());
    }
}
