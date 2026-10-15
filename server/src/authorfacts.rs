//! A line about an author, from Wikidata: who the person was, when they
//! lived, and where Wikipedia has more. The library shows it above the
//! books when they are filtered on one author.
//!
//! Only a person found for certain is shown: a human of the author's name,
//! in some language, whose occupation is writing, and either the only one of
//! the name that Wikidata's search returns or by far the best known of them.
//! Anything less gives no line at all.
//!
//! What is found is kept in `author_facts`, one row per author on the
//! instance, and asked for again only now and then. A page never waits for
//! Wikidata: the row is read, and a missing or old one is looked up in the
//! background, one request a second at most, for the next time.

use crate::auth::AuthUser;
use crate::db::{now_ts, ts_in_hours};
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use unicode_normalization::UnicodeNormalization;

const API: &str = "https://www.wikidata.org/w/api.php";
const TIMEOUT: Duration = Duration::from_secs(10);
/// How long what was found is trusted before it is asked for again.
const OK_HOURS: i64 = 24 * 90;
const NONE_HOURS: i64 = 24 * 30;
const ERROR_HOURS: i64 = 1;
/// The interface's languages, for names, descriptions and Wikipedia.
const LANGS: [&str; 7] = ["en", "sv", "fi", "de", "fr", "es", "eo"];
/// Occupations that make a person an author for this purpose: writer,
/// novelist, poet, author, playwright, essayist, short story writer,
/// children's writer, science fiction writer, journalist, translator,
/// screenwriter, literary critic, philosopher, historian.
const WRITERS: [&str; 15] = [
    "Q36180", "Q6625963", "Q49757", "Q482980", "Q214917", "Q11774202", "Q15949613", "Q4853732",
    "Q18844224", "Q1930187", "Q333634", "Q28389", "Q4263842", "Q4964182", "Q201788",
];

pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "author_facts_enabled", false).await.unwrap_or(false)
}

/// A person's identity as the web reader writes it (lib/library/authors.ts):
/// lowercase, without diacritics, and anything but letters and digits as
/// single spaces.
pub fn key_of(name: &str) -> String {
    let folded: String = name.nfd().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).collect::<String>().to_lowercase();
    folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// A person found for certain, in every language of the interface.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Facts {
    pub qid: String,
    pub born: Option<i32>,
    pub died: Option<i32>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub descriptions: BTreeMap<String, String>,
    /// Wikipedia article titles, by language.
    #[serde(default)]
    pub wiki: BTreeMap<String, String>,
}

/// The year of a date claim, such as P569 (born): "+1858-11-20T00:00:00Z".
fn year(entity: &Value, property: &str) -> Option<i32> {
    let time = entity.pointer(&format!("/claims/{property}/0/mainsnak/datavalue/value/time"))?.as_str()?;
    let (sign, rest) = time.split_at(1);
    let year: i32 = rest.split('-').next()?.parse().ok()?;
    Some(if sign == "-" { -year } else { year })
}

fn claim_ids<'a>(entity: &'a Value, property: &str) -> impl Iterator<Item = &'a str> {
    entity
        .pointer(&format!("/claims/{property}"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.pointer("/mainsnak/datavalue/value/id").and_then(Value::as_str))
}

/// An author with others of the same name is taken only when found in at
/// least this many Wikipedias, and this many times as many as the next.
const KNOWN: usize = 10;
const AHEAD: usize = 3;

/// How many Wikipedias have an article on the entity.
fn articles(entity: &Value) -> usize {
    entity
        .get("sitelinks")
        .and_then(Value::as_object)
        .map(|m| m.keys().filter(|site| site.ends_with("wiki") && *site != "commonswiki" && *site != "specieswiki").count())
        .unwrap_or(0)
}

/// Every name an entity has, in any language: labels and aliases.
fn names(entity: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(labels) = entity.get("labels").and_then(Value::as_object) {
        out.extend(labels.values().filter_map(|l| l.get("value").and_then(Value::as_str)).map(str::to_string));
    }
    if let Some(aliases) = entity.get("aliases").and_then(Value::as_object) {
        for list in aliases.values().filter_map(Value::as_array) {
            out.extend(list.iter().filter_map(|a| a.get("value").and_then(Value::as_str)).map(str::to_string));
        }
    }
    out
}

fn by_language(entity: &Value, field: &str) -> BTreeMap<String, String> {
    entity
        .get(field)
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(lang, v)| v.get("value").and_then(Value::as_str).map(|s| (lang.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Out of the entities Wikidata's search found, the one person who is
/// certainly the author: the only human called by the name, and one who
/// writes. None when there is no such person, or more than one of the name.
pub fn choose(entities: &Value, key: &str) -> Option<Facts> {
    let all = entities.get("entities")?.as_object()?;
    // Everyone of the name, writer or not, the best known first (by how many
    // Wikipedias have an article): with two people called John Smith, a
    // single writer among them is no proof that he is the one.
    let mut named: Vec<&Value> = all
        .values()
        .filter(|e| claim_ids(e, "P31").any(|id| id == "Q5") && names(e).iter().any(|n| key_of(n) == key))
        .collect();
    named.sort_by_key(|e| std::cmp::Reverse(articles(e)));
    let person = *named.first()?;
    if !claim_ids(person, "P106").any(|id| WRITERS.contains(&id)) {
        return None;
    }
    // Others of the name are no rivals to an author far better known than
    // them: Mary Shelley has 133 articles, the seven others of her name none.
    if let Some(next) = named.get(1) {
        let (first, second) = (articles(person), articles(next));
        if first < KNOWN || first < second * AHEAD {
            return None;
        }
    }
    let wiki = person
        .get("sitelinks")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(site, v)| {
                    let lang = site.strip_suffix("wiki").filter(|l| LANGS.contains(l))?;
                    Some((lang.to_string(), v.get("title")?.as_str()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Facts {
        qid: person.get("id")?.as_str()?.to_string(),
        born: year(person, "P569"),
        died: year(person, "P570"),
        labels: by_language(person, "labels"),
        descriptions: by_language(person, "descriptions"),
        wiki,
    })
}

async fn get(state: &AppState, params: &[(&str, &str)]) -> anyhow::Result<Value> {
    let ua = format!("Legejo/{} (https://github.com/jularboolean/legejo)", env!("CARGO_PKG_VERSION"));
    let response = state
        .http
        .get(API)
        .query(params)
        .header(header::USER_AGENT, ua)
        .timeout(TIMEOUT)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

/// Ask Wikidata who the author is. Ok(None) when no one is certain.
async fn look_up(state: &AppState, name: &str, key: &str) -> anyhow::Result<Option<Facts>> {
    let mut ids: Vec<String> = Vec::new();
    for language in ["en", "sv"] {
        let found = get(
            state,
            &[("action", "wbsearchentities"), ("search", name), ("language", language), ("type", "item"), ("limit", "10"), ("format", "json")],
        )
        .await?;
        for hit in found.get("search").and_then(Value::as_array).into_iter().flatten() {
            if let Some(id) = hit.get("id").and_then(Value::as_str) {
                if !ids.iter().any(|i| i == id) {
                    ids.push(id.to_string());
                }
            }
        }
        if !ids.is_empty() {
            break;
        }
    }
    if ids.is_empty() {
        return Ok(None);
    }
    // Every sitelink, not just those of the interface's languages: how many
    // there are tells a well-known author from others of the same name.
    let languages = LANGS.join("|");
    let entities = get(
        state,
        &[
            ("action", "wbgetentities"),
            ("ids", &ids.join("|")),
            ("props", "labels|aliases|descriptions|claims|sitelinks"),
            ("languages", &languages),
            ("format", "json"),
        ],
    )
    .await?;
    Ok(choose(&entities, key))
}

/// Look the author up and keep what was found. A failure keeps what an
/// earlier look-up found, if anything.
async fn refresh(state: &AppState, key: &str, name: &str) {
    let (row_state, facts) = match look_up(state, name, key).await {
        Ok(Some(facts)) => ("ok", Some(facts)),
        Ok(None) => ("none", None),
        Err(e) => {
            tracing::info!("author facts: could not look up \"{name}\": {e:#}");
            let known: Option<String> = sqlx::query_scalar("SELECT state FROM author_facts WHERE author_key = $1")
                .bind(key)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
            if known.as_deref() == Some("ok") {
                return;
            }
            ("error", None)
        }
    };
    let qid = facts.as_ref().map(|f| f.qid.clone());
    let facts = facts.map(|f| serde_json::to_string(&f).unwrap_or_default());
    let saved = sqlx::query(
        "INSERT INTO author_facts (author_key, name, state, qid, facts, fetched_at) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (author_key) DO UPDATE SET name = excluded.name, state = excluded.state, qid = excluded.qid,
             facts = excluded.facts, fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(name)
    .bind(row_state)
    .bind(qid)
    .bind(facts)
    .bind(now_ts())
    .execute(&state.db)
    .await;
    if let Err(e) = saved {
        tracing::warn!("author facts: could not keep what was found for \"{name}\": {e}");
    }
}

type Job = (String, String);
static QUEUE: OnceLock<tokio::sync::mpsc::UnboundedSender<Job>> = OnceLock::new();
static WAITING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Look the author up in the background, once, whatever asks how often.
fn enqueue(state: &AppState, key: &str, name: &str) {
    if cfg!(test) {
        return;
    }
    let waiting = WAITING.get_or_init(Default::default);
    if !waiting.lock().map(|mut w| w.insert(key.to_string())).unwrap_or(false) {
        return;
    }
    let queue = QUEUE.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        let state = state.clone();
        tokio::spawn(async move {
            while let Some((key, name)) = rx.recv().await {
                refresh(&state, &key, &name).await;
                if let Some(w) = WAITING.get() {
                    if let Ok(mut w) = w.lock() {
                        w.remove(&key);
                    }
                }
                // Wikimedia asks for no more than this from a client like this.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        tx
    });
    let _ = queue.send((key.to_string(), name.to_string()));
}

#[derive(Deserialize)]
pub struct LineQuery {
    name: String,
    #[serde(default)]
    lang: Option<String>,
}

fn pick<'a>(map: &'a BTreeMap<String, String>, lang: &str) -> Option<(&'a str, &'a String)> {
    map.get_key_value(lang).or_else(|| map.get_key_value("en")).map(|(l, v)| (l.as_str(), v))
}

fn answer(body: Value, cache: &'static str) -> Response {
    ([(header::CACHE_CONTROL, cache)], Json(body)).into_response()
}

/// `GET /api/authors/facts?name=Selma Lagerlöf&lang=sv`: the line about an
/// author, when there is one. `state` says why there is none: `off` (not
/// turned on), `pending` (being looked up), `none`, or `hidden` (the user
/// said it was the wrong person).
pub async fn line(State(state): State<AppState>, user: AuthUser, Query(q): Query<LineQuery>) -> Result<Response, Response> {
    if !enabled(&state).await {
        return Ok(answer(json!({ "state": "off" }), "private, no-store"));
    }
    let name = q.name.split_whitespace().collect::<Vec<_>>().join(" ");
    let key = key_of(&name);
    if key.is_empty() || name.chars().count() > 200 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "no name" }))).into_response());
    }
    let hidden: Option<i64> = sqlx::query_scalar("SELECT 1 FROM author_facts_hidden WHERE user_id = $1 AND author_key = $2")
        .bind(user.0.id)
        .bind(&key)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if hidden.is_some() {
        return Ok(answer(json!({ "state": "hidden" }), "private, no-store"));
    }
    let row: Option<(String, Option<String>, String)> =
        sqlx::query_as("SELECT state, facts, fetched_at FROM author_facts WHERE author_key = $1")
            .bind(&key)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    let Some((row_state, facts, fetched_at)) = row else {
        enqueue(&state, &key, &name);
        return Ok(answer(json!({ "state": "pending" }), "private, no-store"));
    };
    let lasts = match row_state.as_str() {
        "ok" => OK_HOURS,
        "none" => NONE_HOURS,
        _ => ERROR_HOURS,
    };
    if fetched_at < ts_in_hours(-lasts) {
        enqueue(&state, &key, &name);
    }
    let facts: Option<Facts> = facts.and_then(|f| serde_json::from_str(&f).ok());
    let Some(facts) = facts.filter(|_| row_state == "ok") else {
        return Ok(answer(json!({ "state": if row_state == "error" { "pending" } else { "none" } }), "private, no-store"));
    };
    let lang = q.lang.as_deref().filter(|l| LANGS.contains(l)).unwrap_or("en");
    let label = pick(&facts.labels, lang).map(|(_, v)| v.clone()).unwrap_or(name);
    let description = pick(&facts.descriptions, lang).map(|(_, v)| v.clone());
    let wikipedia = pick(&facts.wiki, lang).map(|(l, title)| {
        let path: String = url::form_urlencoded::byte_serialize(title.replace(' ', "_").as_bytes()).collect();
        json!({ "lang": l, "url": format!("https://{l}.wikipedia.org/wiki/{path}") })
    });
    Ok(answer(
        json!({
            "state": "ok",
            "key": key,
            "qid": facts.qid,
            "name": label,
            "born": facts.born,
            "died": facts.died,
            "description": description,
            "wikipedia": wikipedia,
        }),
        "private, max-age=86400",
    ))
}

#[derive(Deserialize)]
pub struct HideRequest {
    name: String,
}

/// `POST /api/authors/facts/hidden`: the line is about the wrong person;
/// not shown to this user again.
pub async fn hide(State(state): State<AppState>, user: AuthUser, Json(req): Json<HideRequest>) -> Result<StatusCode, Response> {
    let key = key_of(&req.name);
    if key.is_empty() {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "no name" }))).into_response());
    }
    sqlx::query(
        "INSERT INTO author_facts_hidden (user_id, author_key, created_at) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, author_key) DO NOTHING",
    )
    .bind(user.0.id)
    .bind(&key)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}
