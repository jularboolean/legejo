//! How long a book takes to read: the length of its text, counted once per
//! file by a worker, at a reading pace. The pace is the reader's own when
//! the web reader or their Kobo has timed books they finished, and otherwise
//! a common one. The web reader's time is kept here too (`reader_time`).

use crate::booktext;
use crate::books::book_path;
use crate::formats::Format;
use crate::progress;
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Characters a minute for a reader whose own pace is not known: about 190
/// words of Swedish or English prose.
pub const COMMON_PACE: i64 = 1100;
/// The most one report from the web reader may add: it sends what it has
/// counted every minute or so, and more is kept on the device only while
/// the network is gone.
const MOST_SECONDS: i64 = 6 * 3600;
/// A finished book counts towards the reader's own pace when the web reader
/// and the Kobo together timed at least this much of it, at a pace between these two: a book read
/// mostly elsewhere, or left open on the device, tells nothing.
const LEAST_MINUTES: i64 = 20;
const SLOWEST: i64 = 300;
const FASTEST: i64 = 3000;
/// How long the worker sleeps when there is nothing to count.
const IDLE: Duration = Duration::from_secs(10);

/// The length of a text as it reads: each run of white space one character.
pub fn chars(sections: &[booktext::Section]) -> i64 {
    let mut count = 0i64;
    for section in sections {
        let mut words = 0i64;
        for word in section.text.split_whitespace() {
            count += word.chars().count() as i64;
            words += 1;
        }
        // The spaces between the words, and one between sections.
        count += words;
    }
    (count - 1).max(0)
}

/// Count the books whose text has not been counted since their file last
/// changed, a few at a time. Returns how many were counted.
pub(crate) async fn count_pending(state: &AppState, limit: i64) -> anyhow::Result<usize> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, uuid, COALESCE(file_sha256, '') FROM books
         WHERE format = 'epub' AND (text_chars_sha256 IS NULL OR text_chars_sha256 != COALESCE(file_sha256, ''))
         ORDER BY id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let done = rows.len();
    for (id, uuid, sha) in rows {
        let path = book_path(state, &uuid, Format::Epub);
        state.storage.fetch(&path).await;
        let counted = tokio::task::spawn_blocking(move || -> anyhow::Result<i64> {
            let bytes = std::fs::read(path)?;
            Ok(chars(&booktext::sections(bytes)?))
        })
        .await?;
        // A file that cannot be read has no length, and is not tried again
        // until it changes.
        let counted = counted.unwrap_or_else(|e| {
            tracing::debug!("reading time: book {id} could not be read: {e:#}");
            0
        });
        sqlx::query("UPDATE books SET text_chars = $1, text_chars_sha256 = $2 WHERE id = $3")
            .bind(counted)
            .bind(&sha)
            .bind(id)
            .execute(&state.db)
            .await?;
    }
    Ok(done)
}

/// Counts new and changed books as they come. Two processes may count the
/// same book; both write the same.
pub async fn worker(state: AppState) {
    loop {
        let busy = match count_pending(&state, 20).await {
            Ok(n) => n > 0,
            Err(e) => {
                tracing::warn!("reading time: {e:#}");
                false
            }
        };
        if !busy {
            tokio::time::sleep(IDLE).await;
        }
    }
}

/// How long a book takes the reader, for the book's page and the reader.
#[derive(Serialize, Debug, PartialEq)]
pub struct ReadingTime {
    /// The length of the text, as `chars` counts it.
    pub chars: i64,
    /// Characters a minute.
    pub pace: i64,
    /// The pace is the reader's own, from this many books the web reader or
    /// their Kobo timed; 0 for the common pace.
    pub paced_by: i64,
    /// The whole book at that pace.
    pub minutes: i64,
}

/// The reader's own pace, from the finished books the web reader or their
/// Kobo timed, with how many books it comes from; None when there are none
/// to go by.
pub async fn own_pace(state: &AppState, user_id: i64) -> anyhow::Result<Option<(i64, i64)>> {
    let rows: Vec<(i64, Option<i64>, i64)> = sqlx::query_as(&format!(
        "SELECT b.text_chars, {minutes}, COALESCE(rt.seconds, 0) FROM books b
         {joins}
         LEFT JOIN reader_time rt ON rt.book_id = b.id AND rt.user_id = $1
         WHERE b.owner_id = $1 AND b.text_chars > 0 AND (b.read_at IS NOT NULL OR {percent} >= 0.97)",
        minutes = progress::kobo_minutes(state.backend),
        percent = progress::progress_percent(state.backend),
        joins = progress::PROGRESS_JOINS,
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(pace_of(rows.into_iter().map(|(chars, kobo, seconds)| (chars, kobo.unwrap_or(0).max(0) + seconds / 60))))
}

/// Minutes the web reader has timed for the reader in a book.
pub async fn reader_minutes(state: &AppState, user_id: i64, book_id: i64) -> anyhow::Result<i64> {
    let seconds: Option<i64> = sqlx::query_scalar("SELECT seconds FROM reader_time WHERE user_id = $1 AND book_id = $2")
        .bind(user_id)
        .bind(book_id)
        .fetch_optional(&state.db)
        .await?;
    Ok(seconds.unwrap_or(0) / 60)
}

#[derive(Deserialize)]
pub struct AddTime {
    seconds: i64,
}

/// The web reader's report of time spent reading a book since the last one.
pub async fn add_reader_time(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<AddTime>,
) -> Result<StatusCode, Response> {
    if !(1..=MOST_SECONDS).contains(&req.seconds) {
        let msg = format!("seconds must be between 1 and {MOST_SECONDS}");
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response());
    }
    // Only for a book the user owns; no row written means no such book.
    let added = sqlx::query(
        "INSERT INTO reader_time (user_id, book_id, seconds, updated_at)
         SELECT $1, id, $3, $4 FROM books WHERE id = $2 AND owner_id = $1
         ON CONFLICT (user_id, book_id)
         DO UPDATE SET seconds = reader_time.seconds + excluded.seconds, updated_at = excluded.updated_at",
    )
    .bind(user.0.id)
    .bind(id)
    .bind(req.seconds)
    .bind(crate::db::now_ts())
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("internal error: {e:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
    })?;
    if added.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response());
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The pace over the books that count, as characters a minute, and how
/// many they are.
fn pace_of(books: impl Iterator<Item = (i64, i64)>) -> Option<(i64, i64)> {
    let (mut chars, mut minutes, mut count) = (0i64, 0i64, 0i64);
    for (c, m) in books {
        if m < LEAST_MINUTES || c / m < SLOWEST || c / m > FASTEST {
            continue;
        }
        chars += c;
        minutes += m;
        count += 1;
    }
    (count > 0).then(|| (chars / minutes, count))
}

/// A book of `chars` characters for this reader; None when its length is
/// not known.
pub async fn for_reader(state: &AppState, user_id: i64, chars: i64) -> Option<ReadingTime> {
    if chars <= 0 {
        return None;
    }
    let own = own_pace(state, user_id).await.unwrap_or_else(|e| {
        tracing::warn!("reading time: own pace: {e:#}");
        None
    });
    let (pace, paced_by) = own.unwrap_or((COMMON_PACE, 0));
    Some(ReadingTime { chars, pace, paced_by, minutes: (chars + pace / 2) / pace })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(text: &str) -> booktext::Section {
        booktext::Section { title: None, text: text.to_string(), start: 0.0, end: 1.0 }
    }

    #[test]
    fn white_space_counts_as_one_character() {
        assert_eq!(chars(&[section("Det var  en\n\ngång.")]), "Det var en gång.".chars().count() as i64);
        assert_eq!(chars(&[section("Ett."), section("  Två. ")]), "Ett. Två.".chars().count() as i64);
        assert_eq!(chars(&[section("Åäö ĉu")]), 6);
        assert_eq!(chars(&[]), 0);
    }

    #[test]
    fn a_pace_comes_from_the_books_timed_long_enough_and_believably() {
        // 420 000 characters in 429 minutes, and 100 000 in 100.
        assert_eq!(pace_of([(420_000, 429), (100_000, 100)].into_iter()), Some((520_000 / 529, 2)));
        // Timed for a few minutes, read in a flash, or left open for days.
        assert_eq!(pace_of([(420_000, 5), (420_000, 30), (420_000, 6000)].into_iter()), None);
        assert_eq!(pace_of(std::iter::empty()), None);
    }
}
