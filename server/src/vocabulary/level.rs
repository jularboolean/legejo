//! The level test: a few words from each frequency band of the reader's
//! books, and from the answers an estimate of how many words they know.

use super::book::set_many;
use super::words::{summary, WordRow, WordSum};
use super::{bad_request, internal, Glossary, LANGUAGE_MAX};
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

/// The frequency bands of the level test, by rank among the words of the
/// reader's books in the language, and the words asked from each.
const LEVEL_BANDS: [usize; 7] = [0, 250, 500, 1000, 2000, 4000, 8000];
const LEVEL_ASK: usize = 8;

/// The words of the reader's books in a language, names aside, commonest
/// first, as far as the level test reaches.
async fn ranked_words(state: &AppState, user: i64, language: &str) -> anyhow::Result<Vec<WordRow>> {
    summary(state, user, language).await?;
    let rows: Vec<WordSum> = sqlx::query_as(&format!(
        "SELECT stem, word, total, books FROM word_sums WHERE user_id = $1 AND language = $2
         ORDER BY total DESC, stem LIMIT {}",
        LEVEL_BANDS[LEVEL_BANDS.len() - 1]
    ))
    .bind(user)
    .bind(language)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|t| WordRow::from_sum(t, false)).collect())
}

/// The bands that the reader's words reach: (from, to) by rank.
fn level_bands(words: usize) -> Vec<(usize, usize)> {
    LEVEL_BANDS.windows(2).filter(|b| b[0] < words).map(|b| (b[0], b[1].min(words))).collect()
}

#[derive(Deserialize)]
pub struct LevelQuery {
    language: String,
}

#[derive(Serialize)]
pub struct LevelWord {
    pub stem: String,
    pub word: String,
    /// Shown once the reader has said they know the word, to check it.
    pub english: Option<String>,
}

#[derive(Serialize)]
pub struct LevelBand {
    pub from: usize,
    pub to: usize,
    pub words: Vec<LevelWord>,
}

/// GET /api/vocabulary/level?language=: words to ask in a short level test,
/// a few picked at random from each frequency band of the reader's books.
pub async fn level_sample(State(state): State<AppState>, user: AuthUser, Query(q): Query<LevelQuery>) -> Response {
    use rand::seq::SliceRandom;
    let ranked = match ranked_words(&state, user.0.id, &q.language).await {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let glossary = Glossary::for_language(&state, &q.language);
    let mut rng = rand::thread_rng();
    let bands: Vec<LevelBand> = level_bands(ranked.len())
        .into_iter()
        .map(|(from, to)| {
            let mut words: Vec<LevelWord> = ranked[from..to]
                .choose_multiple(&mut rng, LEVEL_ASK)
                .map(|w| LevelWord { stem: w.stem.clone(), word: w.word.clone(), english: glossary.meaning(&w.word, &w.stem).english })
                .collect();
            words.shuffle(&mut rng);
            LevelBand { from, to, words }
        })
        .collect();
    Json(json!({ "words": ranked.len(), "bands": bands })).into_response()
}

#[derive(Deserialize)]
pub struct LevelAnswer {
    stem: String,
    known: bool,
}

#[derive(Deserialize)]
pub struct LevelAnswers {
    language: String,
    answers: Vec<LevelAnswer>,
}

/// POST /api/vocabulary/level: the answers of a level test. In each band
/// the reader is taken to know as large a share of the words as they knew
/// of those asked, the commonest first; the words answered are ticked or
/// left as answered. Words ticked before stay ticked.
pub async fn level_apply(State(state): State<AppState>, user: AuthUser, Json(a): Json<LevelAnswers>) -> Response {
    if a.answers.len() > 200 || a.language.is_empty() || a.language.len() > LANGUAGE_MAX {
        return bad_request("a language and at most 200 answers");
    }
    let ranked = match ranked_words(&state, user.0.id, &a.language).await {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    let rank: HashMap<&str, usize> = ranked.iter().enumerate().map(|(i, w)| (w.stem.as_str(), i)).collect();
    let answer: HashMap<&str, bool> = a.answers.iter().map(|x| (x.stem.as_str(), x.known)).collect();
    let mut tick: Vec<&String> = Vec::new();
    let mut estimate = 0usize;
    for (from, to) in level_bands(ranked.len()) {
        let asked: Vec<bool> = answer.iter().filter(|(s, _)| rank.get(*s).is_some_and(|r| (from..to).contains(r))).map(|(_, k)| *k).collect();
        if asked.is_empty() {
            continue;
        }
        let share = asked.iter().filter(|k| **k).count() as f64 / asked.len() as f64;
        let take = ((to - from) as f64 * share).round() as usize;
        estimate += take;
        tick.extend(ranked[from..from + take].iter().map(|w| &w.stem).filter(|s| answer.get(s.as_str()) != Some(&false)));
    }
    tick.extend(a.answers.iter().filter(|x| x.known && rank.contains_key(x.stem.as_str())).map(|x| &x.stem));
    tick.sort();
    tick.dedup();
    match set_many(&state, user.0.id, &a.language, &tick, true).await {
        Ok(()) => Json(json!({ "estimate": estimate, "ticked": tick.len() })).into_response(),
        Err(e) => internal(e),
    }
}
