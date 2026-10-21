//! Looking a word up while reading. The reader asks here, and the server
//! asks the dictionary, so the reader's address never reaches it:
//!
//! - Esperanto: Reta Vortaro (reta-vortaro.de), the open Esperanto
//!   dictionary, with the word's translations into the reader's own language
//!   and English, and a link to its article. A word as it stands in the text
//!   ("ĉambrojn", "legis") is tried as the dictionary has it ("ĉambro",
//!   "legi") when it is not there as it stands.
//! - Every language, and Esperanto when Reta Vortaro has nothing: the English
//!   Wiktionary's definitions (CC BY-SA), which cover words of many
//!   languages.
//!
//! Off until an admin turns it on: the server then sends the words looked
//! up to those sites. Answers are kept in memory for a day.

use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Whether an admin has turned the dictionary on.
pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "dictionary_enabled", false).await.unwrap_or(false)
}

const USER_AGENT: &str = concat!("Legejo/", env!("CARGO_PKG_VERSION"), " (+https://github.com/jularboolean/legejo)");
/// Reta Vortaro's own web client sends this with every search: "a simple
/// fence against robots", not a secret.
const REVO_KEY: &str = "1887";
const KEEP: Duration = Duration::from_secs(24 * 3600);
const KEEP_AT_MOST: usize = 5000;

#[derive(Serialize, Clone)]
pub struct Entry {
    /// The word as the dictionary has it.
    pub headword: String,
    /// What it means, in English (Wiktionary).
    pub definitions: Vec<String>,
    /// What it is in other languages (Reta Vortaro), the reader's first.
    pub translations: Vec<Translation>,
    /// The whole article.
    pub link: String,
}

#[derive(Serialize, Clone)]
pub struct Translation {
    pub language: String,
    pub words: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct Answer {
    pub word: String,
    /// "revo" or "wiktionary"; absent when nothing was found.
    pub source: Option<&'static str>,
    pub entries: Vec<Entry>,
}

#[derive(Deserialize)]
pub struct Params {
    word: String,
    /// The book's language.
    language: Option<String>,
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Answer)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<String, (Instant, Answer)>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// GET /api/dictionary?word=…&language=…
pub async fn lookup(State(state): State<AppState>, user: crate::auth::AuthUser, Query(params): Query<Params>) -> Response {
    if !enabled(&state).await {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "the dictionary is off" }))).into_response();
    }
    let word = clean(&params.word);
    if word.is_empty() || word.chars().count() > 60 {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "one word" }))).into_response();
    }
    let language = params.language.as_deref().map(primary).unwrap_or("en").to_string();
    // Esperanto as the dictionary writes it, whichever way it was typed.
    let word = if language == "eo" { crate::esperanto::supersigned(&word).unwrap_or(word).to_lowercase() } else { word };
    let locale: Option<String> =
        sqlx::query_scalar("SELECT locale FROM users WHERE id = $1").bind(user.0.id).fetch_optional(&state.db).await.ok().flatten().flatten();
    let reader = locale.as_deref().map(primary).unwrap_or("en").to_string();
    let key = format!("{language}|{reader}|{word}");
    if let Some((at, answer)) = cache().lock().unwrap().get(&key) {
        if at.elapsed() < KEEP {
            return Json(answer.clone()).into_response();
        }
    }
    match look_up(&state, &word, &language, &reader).await {
        Ok(answer) => {
            let mut kept = cache().lock().unwrap();
            if kept.len() >= KEEP_AT_MOST {
                kept.retain(|_, (at, _)| at.elapsed() < KEEP);
                if kept.len() >= KEEP_AT_MOST {
                    kept.clear();
                }
            }
            kept.insert(key, (Instant::now(), answer.clone()));
            Json(answer).into_response()
        }
        Err(e) => {
            tracing::warn!("dictionary: \"{word}\" ({language}) could not be looked up: {e:#}");
            (StatusCode::BAD_GATEWAY, Json(json!({ "error": "the dictionary could not be reached" }))).into_response()
        }
    }
}

/// "en-GB" → "en".
fn primary(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag).trim()
}

/// The word without the punctuation a selection brings along.
fn clean(word: &str) -> String {
    word.trim().trim_matches(|c: char| !c.is_alphanumeric() && !['\'', '’', '-'].contains(&c)).trim_matches(['\'', '’', '-']).to_string()
}

async fn look_up(state: &AppState, word: &str, language: &str, reader: &str) -> anyhow::Result<Answer> {
    if language == "eo" {
        let word = word.to_string();
        let revo = async {
            for candidate in esperanto_forms(&word) {
                let entries = revo(state, &candidate, reader).await?;
                if !entries.is_empty() {
                    return anyhow::Ok(entries);
                }
            }
            anyhow::Ok(Vec::new())
        }
        .await;
        match revo {
            Ok(entries) if !entries.is_empty() => return Ok(Answer { word, source: Some("revo"), entries }),
            Ok(_) => {}
            Err(e) => tracing::warn!("dictionary: Reta Vortaro did not answer for \"{word}\": {e:#}; Wiktionary instead"),
        }
        for candidate in esperanto_forms(&word) {
            let entries = wiktionary(state, &candidate, "eo").await?;
            if !entries.is_empty() {
                return Ok(Answer { word, source: Some("wiktionary"), entries });
            }
        }
        return Ok(Answer { word, source: None, entries: Vec::new() });
    }
    let mut entries = wiktionary(state, word, language).await?;
    if entries.is_empty() && word.chars().any(char::is_uppercase) {
        entries = wiktionary(state, &word.to_lowercase(), language).await?;
    }
    let source = (!entries.is_empty()).then_some("wiktionary");
    Ok(Answer { word: word.to_string(), source, entries })
}

/// An Esperanto word as it stands, then as a dictionary has it: without the
/// plural and accusative endings, a verb in the infinitive, a participle as
/// its verb. Esperanto's regular endings make this reliable enough.
pub fn esperanto_forms(word: &str) -> Vec<String> {
    let mut forms = vec![word.to_string()];
    let mut push = |f: String| {
        if f.chars().count() >= 2 && !forms.contains(&f) {
            forms.push(f);
        }
    };
    let mut bare = word.to_string();
    for ending in ["n", "j"] {
        if bare.ends_with(ending) && bare.len() > 3 {
            bare.truncate(bare.len() - ending.len());
        }
    }
    push(bare.clone());
    for participle in ["anta", "inta", "onta", "ata", "ita", "ota", "ante", "inte", "onte", "ate", "ite", "ote", "anto", "into", "onto", "ato", "ito", "oto"] {
        if let Some(root) = bare.strip_suffix(participle) {
            push(format!("{root}i"));
        }
    }
    for ending in ["as", "is", "os", "us", "u"] {
        if let Some(root) = bare.strip_suffix(ending) {
            push(format!("{root}i"));
        }
    }
    // An adverb or adjective whose article is under another word class.
    if let Some(root) = bare.strip_suffix('e').or_else(|| bare.strip_suffix('a')).or_else(|| bare.strip_suffix('o')) {
        for class in ["o", "a", "i"] {
            push(format!("{root}{class}"));
        }
    }
    forms.truncate(5);
    forms
}

/// Reta Vortaro's entries for exactly this headword, with translations into
/// the reader's language and English. Its search takes a form sent as
/// multipart, as its own web client sends it.
async fn revo(state: &AppState, word: &str, reader: &str) -> anyhow::Result<Vec<Entry>> {
    let boundary = format!("legejo{}", crate::books::new_uuid());
    let mut body = String::new();
    for (name, value) in [("sxlosilo", REVO_KEY), ("sercxata", word)] {
        body.push_str(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"));
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    let languages = if reader == "en" { "en".to_string() } else { format!("{reader},en") };
    let base = state.settings.dictionary.revo.trim_end_matches('/');
    let response = state
        .http
        .post(format!("{base}/cgi-bin/sercxu-json-2p.pl"))
        .header("user-agent", USER_AGENT)
        .header("accept", "application/json")
        .header("accept-language", languages)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .send()
        .await?
        .error_for_status()?;
    let found: Value = response.json().await?;
    let rows = found.get("eo").and_then(Value::as_array).cloned().unwrap_or_default();
    // [mark, headword, language, translation, …], one row per translation.
    let mut entries: Vec<(String, Entry)> = Vec::new();
    for row in rows {
        let Some(row) = row.as_array() else { continue };
        let text = |i: usize| row.get(i).and_then(Value::as_str).unwrap_or("").to_string();
        let (mark, headword, language, translation) = (text(0), text(1), text(2), text(3));
        if mark.is_empty() || headword.to_lowercase() != word {
            continue;
        }
        let at = match entries.iter().position(|(m, _)| *m == mark) {
            Some(at) => at,
            None => {
                let article = mark.split('.').next().unwrap_or(&mark);
                entries.push((
                    mark.clone(),
                    Entry {
                        headword: headword.clone(),
                        definitions: Vec::new(),
                        translations: Vec::new(),
                        link: format!("https://reta-vortaro.de/revo/art/{article}.html#{mark}"),
                    },
                ));
                entries.len() - 1
            }
        };
        if language.is_empty() || translation.is_empty() {
            continue;
        }
        let entry = &mut entries[at].1;
        match entry.translations.iter_mut().find(|t| t.language == language) {
            Some(t) if !t.words.contains(&translation) => t.words.push(translation),
            Some(_) => {}
            None => entry.translations.push(Translation { language, words: vec![translation] }),
        }
    }
    let mut entries: Vec<Entry> = entries.into_iter().map(|(_, e)| e).collect();
    for entry in &mut entries {
        // The reader's language first.
        entry.translations.sort_by_key(|t| t.language != reader);
    }
    Ok(entries)
}

/// The English Wiktionary's definitions of the word in the language.
async fn wiktionary(state: &AppState, word: &str, language: &str) -> anyhow::Result<Vec<Entry>> {
    let base = state.settings.dictionary.wiktionary.trim_end_matches('/');
    let title: String = url::form_urlencoded::byte_serialize(word.replace(' ', "_").as_bytes()).collect();
    let response = state
        .http
        .get(format!("{base}/api/rest_v1/page/definition/{title}"))
        .header("user-agent", USER_AGENT)
        .header("accept", "application/json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let found: Value = response.error_for_status()?.json().await?;
    let Some(senses) = found.get(language).and_then(Value::as_array) else { return Ok(Vec::new()) };
    let mut entries = Vec::new();
    for sense in senses {
        let part = sense.get("partOfSpeech").and_then(Value::as_str).unwrap_or("");
        let definitions: Vec<String> = sense
            .get("definitions")
            .and_then(Value::as_array)
            .map(|ds| {
                ds.iter()
                    .filter_map(|d| d.get("definition").and_then(Value::as_str))
                    .map(plain)
                    .filter(|d| !d.is_empty())
                    .take(6)
                    .collect()
            })
            .unwrap_or_default();
        if definitions.is_empty() {
            continue;
        }
        let language_name = sense.get("language").and_then(Value::as_str).unwrap_or("");
        entries.push(Entry {
            headword: if part.is_empty() { word.to_string() } else { format!("{word} ({})", part.to_lowercase()) },
            definitions,
            translations: Vec::new(),
            link: format!("https://en.wiktionary.org/wiki/{title}#{}", language_name.replace(' ', "_")),
        });
    }
    entries.truncate(4);
    Ok(entries)
}

/// Wiktionary's definition HTML as plain text.
fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_as_it_stands_is_tried_as_the_dictionary_has_it() {
        assert_eq!(esperanto_forms("ĉambrojn")[..2], ["ĉambrojn", "ĉambro"]);
        assert!(esperanto_forms("legis").contains(&"legi".to_string()));
        assert!(esperanto_forms("legata").contains(&"legi".to_string()));
        assert!(esperanto_forms("rapide").contains(&"rapida".to_string()));
        assert_eq!(esperanto_forms("domo"), ["domo", "doma", "domi"]);
    }

    #[test]
    fn a_selection_is_cut_down_to_its_word() {
        assert_eq!(clean("  «ĉambro», "), "ĉambro");
        assert_eq!(clean("don't."), "don't");
        assert_eq!(clean("(well-known)"), "well-known");
        assert_eq!(primary("en-GB"), "en");
    }

    #[test]
    fn definitions_come_as_plain_text() {
        assert_eq!(plain(r#"A <a href="/wiki/room">room</a>, a &amp; b"#), "A room, a & b");
    }
}
