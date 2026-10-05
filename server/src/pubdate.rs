//! Dates of books: `published` is this edition's date, always stored as
//! "YYYY" or "YYYY-MM-DD"; `first_published` is the year the work first
//! appeared, an integer.
//!
//! EPUB files and Libris write dates every which way ("2013-10-25T00:00:00+00:00",
//! "2023-06", "[2019]", "c1999", calibre's placeholder "0101-01-01"), so
//! everything coming in goes through `normalize`, and the API only accepts
//! the normalized forms.

use crate::AppState;

pub fn current_year() -> i64 {
    time::OffsetDateTime::now_utc().year() as i64
}

/// The leading year (and month, day) out of any common form; None when there
/// is no plausible edition date. A month without a day is dropped to the year.
pub fn normalize(raw: &str) -> Option<String> {
    let s = raw.trim();
    // Skip a short prefix like "[", "c", "©", "cop. ", "ca ".
    let start = s.char_indices().find(|(_, c)| c.is_ascii_digit())?.0;
    if s[..start].chars().count() > 5 {
        return None;
    }
    let rest = &s[start..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() != 4 {
        return None;
    }
    let year: i64 = digits.parse().ok()?;
    if !(1000..=current_year() + 1).contains(&year) {
        return None;
    }
    let after = &rest[4..];
    let mut parts = after.strip_prefix('-').map(|a| a.split(|c: char| !c.is_ascii_digit()));
    let month = parts.as_mut().and_then(|p| p.next()).and_then(|m| m.parse::<u8>().ok());
    let day = parts.as_mut().and_then(|p| p.next()).and_then(|d| d.parse::<u8>().ok());
    if let (Some(m), Some(d)) = (month, day) {
        if let Ok(month) = time::Month::try_from(m) {
            if time::Date::from_calendar_date(year as i32, month, d).is_ok() {
                return Some(format!("{year:04}-{m:02}-{d:02}"));
            }
        }
    }
    Some(format!("{year:04}"))
}

/// Exactly the stored forms: "YYYY" or a real "YYYY-MM-DD".
#[cfg(test)]
fn is_normalized(s: &str) -> bool {
    normalize(s).as_deref() == Some(s)
}

pub fn year_of(published: &str) -> Option<i64> {
    published.get(..4)?.parse().ok()
}

/// A first-publication year: antiquity is fine, the future is not.
pub fn valid_first_year(y: i64) -> bool {
    (-3000..=current_year() + 1).contains(&y)
}

/// Run once at startup: normalize every `published` and fill an empty
/// `first_published` with the edition's year, unless that year is after the
/// author's death (then it is a later edition and the owner must set it).
/// Idempotent, since two processes may run it concurrently during a rolling
/// deploy; marked done in `settings` so it does not run again.
pub async fn backfill(state: AppState) {
    const FLAG: &str = "pubdate_backfill_v1";
    if crate::admin::setting_bool(&state, FLAG, false).await.unwrap_or(false) {
        return;
    }
    let rows: Vec<(i64, Option<String>, Option<i64>, Option<i64>)> =
        match sqlx::query_as("SELECT id, published, first_published, author_death_year FROM books").fetch_all(&state.db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("pubdate backfill skipped: {e}");
                return;
            }
        };
    let (mut normalized, mut cleared, mut guessed) = (0, 0, 0);
    for (id, published, first, died) in rows {
        let new_pub = published.as_deref().and_then(normalize);
        if new_pub != published {
            if new_pub.is_some() {
                normalized += 1;
            } else {
                cleared += 1;
            }
            let _ = sqlx::query("UPDATE books SET published = $1 WHERE id = $2").bind(&new_pub).bind(id).execute(&state.db).await;
        }
        if first.is_none() {
            let year = new_pub.as_deref().and_then(year_of);
            let guess = match (year, died) {
                (Some(y), Some(d)) if y > d => None,
                (y, _) => y,
            };
            if let Some(y) = guess {
                let _ = sqlx::query("UPDATE books SET first_published = CAST($1 AS BIGINT) WHERE id = $2 AND first_published IS NULL")
                    .bind(y.to_string())
                    .bind(id)
                    .execute(&state.db)
                    .await;
                guessed += 1;
            }
        }
    }
    let _ = sqlx::query("INSERT INTO settings (key, value) VALUES ($1, 'true') ON CONFLICT (key) DO UPDATE SET value = 'true'")
        .bind(FLAG)
        .execute(&state.db)
        .await;
    tracing::info!("publication dates: {normalized} normalized, {cleared} cleared (unparseable), {guessed} first-published years guessed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_the_forms_we_meet() {
        assert_eq!(normalize("2013-10-25T00:00:00+00:00").as_deref(), Some("2013-10-25"));
        assert_eq!(normalize("2023-06").as_deref(), Some("2023"));
        assert_eq!(normalize("[2019]").as_deref(), Some("2019"));
        assert_eq!(normalize("c1999").as_deref(), Some("1999"));
        assert_eq!(normalize("cop. 2004").as_deref(), Some("2004"));
        assert_eq!(normalize("1879").as_deref(), Some("1879"));
        assert_eq!(normalize("2021-02-30").as_deref(), Some("2021"), "no such day: the year stands");
        assert_eq!(normalize("0101-01-01T00:00:00+00:00"), None, "calibre's placeholder");
        assert_eq!(normalize("okänt"), None);
        assert_eq!(normalize("Stockholm 1923"), None, "too much before the year");
        assert_eq!(normalize("19th century"), None);
        assert!(is_normalized("2013-10-25"));
        assert!(is_normalized("2013"));
        assert!(!is_normalized("2013-10"));
        assert!(!is_normalized("[2013]"));
        assert_eq!(year_of("2013-10-25"), Some(2013));
        assert!(valid_first_year(-800));
        assert!(!valid_first_year(3000));
    }
}
