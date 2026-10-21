//! Looking words up while reading: off until an admin turns it on, Esperanto
//! in Reta Vortaro with the reader's own language first, other languages in
//! Wiktionary. Both are played by a small server of the test's own. The
//! answers are kept in memory across tests, so every test uses words of its
//! own.

use crate::testutil::{add_user, send, test_app};
use axum::http::{Method, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// The pretend dictionaries; the requests they got.
async fn start_dictionaries() -> (Arc<Mutex<Vec<String>>>, String) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let noted = seen.clone();
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let noted = noted.clone();
        async move {
            let (parts, body) = request.into_parts();
            let body = String::from_utf8_lossy(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).to_string();
            let path = parts.uri.path().to_string();
            let language = parts.headers.get("accept-language").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            let word = body.split("name=\"sercxata\"\r\n\r\n").nth(1).and_then(|r| r.split("\r\n").next()).unwrap_or("").to_string();
            noted.lock().unwrap().push(format!("{} {path} {word} [{language}]", parts.method));
            let answer = match path.as_str() {
                "/cgi-bin/sercxu-json-2p.pl" => match word.as_str() {
                    "ĉambro" => json!({ "eo": [
                        ["cxambr.0o", "ĉambro", "en", "room", ""],
                        ["cxambr.0o", "ĉambro", "sv", "rum", ""],
                        ["cxambr.0o", "ĉambro", "sv", "kammare", ""],
                        ["cxambr.0o", "ĉambro", "", null, null],
                        ["cxambr.0eto", "ĉambreto", "en", "closet", ""]
                    ], "trd": [] }),
                    _ => json!({ "eo": [], "trd": [] }),
                },
                "/api/rest_v1/page/definition/lighthouse" => json!({ "en": [{
                    "partOfSpeech": "Noun", "language": "English",
                    "definitions": [{ "definition": "A <a href=\"/wiki/tower\">tower</a> with a light &amp; a keeper." }]
                }]}),
                "/api/rest_v1/page/definition/%C5%9Dipeto" => json!({ "eo": [{
                    "partOfSpeech": "Noun", "language": "Esperanto",
                    "definitions": [{ "definition": "boat" }]
                }]}),
                _ => return (StatusCode::NOT_FOUND, "").into_response_for_test(),
            };
            (StatusCode::OK, axum::Json(answer)).into_response_for_test()
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (seen, base)
}

trait ForTest {
    fn into_response_for_test(self) -> axum::response::Response;
}
impl<T: axum::response::IntoResponse> ForTest for T {
    fn into_response_for_test(self) -> axum::response::Response {
        self.into_response()
    }
}

/// Alice reads Legejo in Swedish; the dictionary on or off.
async fn setup(base: &str, on: bool) -> (Router, String) {
    let (_, db, mut state) = test_app().await;
    let services = crate::settings::DictionaryServices { revo: base.into(), wiktionary: base.into() };
    state.settings = Arc::new(crate::settings::Settings { dictionary: services, ..crate::settings::Settings::default() });
    let app = crate::router(state);
    let (alice, cookie) = add_user(&db, "alice").await;
    sqlx::query("UPDATE users SET is_admin = 1, locale = 'sv' WHERE id = ?").bind(alice).execute(&db).await.unwrap();
    let body = json!({ "libris_enabled": false, "registration_enabled": false, "dictionary_enabled": on });
    let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&cookie), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["dictionary_enabled"], on);
    (app, cookie)
}

#[tokio::test]
async fn off_by_default_nothing_is_asked() {
    let (seen, base) = start_dictionaries().await;
    let (app, cookie) = setup(&base, false).await;
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&cookie), None).await;
    assert_eq!(config["dictionary"], false);
    let (status, _) = send(&app, Method::GET, "/api/dictionary?word=ĉambro&language=eo", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn esperanto_comes_from_reta_vortaro_with_the_readers_language_first() {
    let (seen, base) = start_dictionaries().await;
    let (app, cookie) = setup(&base, true).await;
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&cookie), None).await;
    assert_eq!(config["dictionary"], true);

    // As it stands in the text, typed in the x-system, with punctuation.
    let (status, v) = send(&app, Method::GET, "/api/dictionary?word=%C2%ABCxambrojn%C2%BB%2C&language=eo", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["source"], "revo");
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "only the word itself, not ĉambreto: {v}");
    assert_eq!(entries[0]["headword"], "ĉambro");
    assert_eq!(entries[0]["link"], "https://reta-vortaro.de/revo/art/cxambr.html#cxambr.0o");
    assert_eq!(entries[0]["translations"], json!([
        { "language": "sv", "words": ["rum", "kammare"] },
        { "language": "en", "words": ["room"] }
    ]));
    let asked = seen.lock().unwrap().clone();
    assert_eq!(asked, vec![
        "POST /cgi-bin/sercxu-json-2p.pl ĉambrojn [sv,en]".to_string(),
        "POST /cgi-bin/sercxu-json-2p.pl ĉambro [sv,en]".to_string(),
    ]);

    // Asked again: from memory.
    send(&app, Method::GET, "/api/dictionary?word=cxambrojn&language=eo", Some(&cookie), None).await;
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn other_languages_and_unknown_esperanto_come_from_wiktionary() {
    let (_, base) = start_dictionaries().await;
    let (app, cookie) = setup(&base, true).await;
    let (status, v) = send(&app, Method::GET, "/api/dictionary?word=Lighthouse&language=en-GB", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["source"], "wiktionary");
    assert_eq!(v["entries"][0]["definitions"], json!(["A tower with a light & a keeper."]));
    assert_eq!(v["entries"][0]["link"], "https://en.wiktionary.org/wiki/lighthouse#English");

    // Not in Reta Vortaro: Wiktionary's Esperanto.
    let (_, v) = send(&app, Method::GET, "/api/dictionary?word=%C5%9Dipeton&language=eo", Some(&cookie), None).await;
    assert_eq!(v["source"], "wiktionary", "{v}");
    assert_eq!(v["entries"][0]["definitions"], json!(["boat"]));

    // Nowhere: an answer with nothing in it, not an error.
    let (status, v) = send(&app, Method::GET, "/api/dictionary?word=xyzzy&language=en", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(v["source"].is_null());
    assert_eq!(v["entries"], json!([]));

    let (status, _) = send(&app, Method::GET, "/api/dictionary?word=%20%2C&language=en", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_dictionary_out_of_reach_is_an_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let nowhere = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (app, cookie) = setup(&nowhere, true).await;
    let (status, _) = send(&app, Method::GET, "/api/dictionary?word=harbourmaster&language=en", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let _: Value = Value::Null;
}
