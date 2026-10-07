//! Wider search: a language model turns a search query into related search
//! terms, which the ordinary search then looks for. Only the query is sent
//! to the model, never anything of the library. Off unless the operator
//! sets LEGEJO_AI_API_KEY; the operator's account pays for the requests.

use crate::db::now_ts;
use crate::AppState;
use serde::Serialize;
use serde_json::{json, Value};

/// Queries longer than this are cut: a search is a few words, not a text.
const MAX_QUERY_CHARS: usize = 200;
const MAX_TERMS: usize = 10;
/// Requests to the model one user may cause in an hour. Queries answered
/// from the kept answers do not count.
const PER_HOUR: i64 = 60;

const INSTRUCTIONS: &str = "You help search a personal library of books. The user gives a search query, in any \
language. Reply with JSON of the form {\"terms\": [...]} holding at most 10 search terms that the title or the \
description of a relevant book is likely to contain: the key words of the query, synonyms, and closely related \
names and concepts. Give every term in Swedish and in English when the two differ. Terms are matched as the \
beginning of a word, so prefer short base forms. Each term is one word, or two words that belong together. \
No explanations.";

#[derive(Serialize, Clone, Copy, Default)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Answered from the kept answers, without a request to the model.
    pub cached: bool,
}

pub struct Expansion {
    pub terms: Vec<String>,
    pub usage: Usage,
}

#[derive(Debug)]
pub enum Failure {
    /// The operator has not set a model up.
    Off,
    TooMany,
    /// The model did not answer, or not with what was asked for.
    Model(String),
}

fn normalize(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase().chars().take(MAX_QUERY_CHARS).collect()
}

/// What the model answered, made safe to search with: short terms of a few
/// words, each once, the query itself first.
fn clean_terms(query: &str, answer: &Value) -> Vec<String> {
    let mut terms: Vec<String> = vec![query.to_string()];
    for term in answer.get("terms").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let term = term.split_whitespace().collect::<Vec<_>>().join(" ");
        let fits = !term.is_empty() && term.chars().count() <= 40 && term.split(' ').count() <= 3;
        if fits && !terms.iter().any(|t| t.to_lowercase() == term.to_lowercase()) {
            terms.push(term);
        }
        if terms.len() > MAX_TERMS {
            break;
        }
    }
    terms
}

/// The search terms for a query: kept ones when the query has been asked
/// before, otherwise from the model.
pub async fn expand(state: &AppState, user_id: i64, query: &str) -> Result<Expansion, Failure> {
    let ai = state.settings.ai.as_ref().ok_or(Failure::Off)?;
    let query = normalize(query);
    let kept: Option<String> = sqlx::query_scalar("SELECT terms FROM search_expansions WHERE query = $1 AND model = $2")
        .bind(&query)
        .bind(&ai.model)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some(terms) = kept.and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok()) {
        return Ok(Expansion { terms, usage: Usage { cached: true, ..Usage::default() } });
    }

    let since = crate::db::ts_in_hours(-1);
    let recent: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage WHERE user_id = $1 AND at > $2")
        .bind(user_id)
        .bind(since)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if recent >= PER_HOUR {
        return Err(Failure::TooMany);
    }

    let response = state
        .http
        .post(format!("{}/chat/completions", ai.base_url))
        .bearer_auth(&ai.key)
        .timeout(std::time::Duration::from_secs(20))
        .json(&json!({
            "model": ai.model,
            "messages": [
                { "role": "system", "content": INSTRUCTIONS },
                { "role": "user", "content": query },
            ],
            "response_format": { "type": "json_object" },
        }))
        .send()
        .await
        .map_err(|e| Failure::Model(format!("no answer: {e}")))?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| Failure::Model(format!("unreadable answer: {e}")))?;
    if !status.is_success() {
        let reason = body["error"]["message"].as_str().unwrap_or("no reason given");
        return Err(Failure::Model(format!("HTTP {status}: {reason}")));
    }
    let usage = Usage {
        prompt_tokens: body["usage"]["prompt_tokens"].as_i64().unwrap_or(0),
        completion_tokens: body["usage"]["completion_tokens"].as_i64().unwrap_or(0),
        cached: false,
    };
    // The request is paid for whatever came back.
    let _ = sqlx::query("INSERT INTO ai_usage (user_id, at, model, prompt_tokens, completion_tokens) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id)
        .bind(now_ts())
        .bind(&ai.model)
        .bind(usage.prompt_tokens)
        .bind(usage.completion_tokens)
        .execute(&state.db)
        .await;
    let answer: Value = body["choices"][0]["message"]["content"]
        .as_str()
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(|| Failure::Model("the answer was not the JSON asked for".to_string()))?;
    let terms = clean_terms(&query, &answer);
    let _ = sqlx::query(
        "INSERT INTO search_expansions (query, model, terms, created_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (query, model) DO NOTHING",
    )
    .bind(&query)
    .bind(&ai.model)
    .bind(serde_json::to_string(&terms).unwrap_or_default())
    .bind(now_ts())
    .execute(&state.db)
    .await;
    Ok(Expansion { terms, usage })
}

/// The use of the model over the last 30 days, for the operator.
#[derive(Serialize)]
pub struct Summary {
    pub model: String,
    pub requests: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
}

pub async fn summary(state: &AppState) -> Option<Summary> {
    let ai = state.settings.ai.as_ref()?;
    let since = crate::db::ts_in_days(-30);
    let (requests, prompt_tokens, completion_tokens): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), CAST(COALESCE(SUM(prompt_tokens), 0) AS BIGINT), CAST(COALESCE(SUM(completion_tokens), 0) AS BIGINT)
         FROM ai_usage WHERE at > $1",
    )
    .bind(since)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0));
    Some(Summary { model: ai.model.clone(), requests, prompt_tokens, completion_tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_made_safe_to_search_with() {
        let answer = json!({ "terms": ["Communism", "kommunism", " marx  ", "a term of far too many words", "", 7, "soviet union"] });
        assert_eq!(clean_terms("kommunism", &answer), ["kommunism", "Communism", "marx", "soviet union"]);
        assert_eq!(clean_terms("x", &json!({ "other": 1 })), ["x"]);
        let many = json!({ "terms": (0..30).map(|n| format!("t{n}")).collect::<Vec<_>>() });
        assert_eq!(clean_terms("q", &many).len(), MAX_TERMS + 1);
        assert_eq!(normalize("  Böcker   OM  Is "), "böcker om is");
    }
}
