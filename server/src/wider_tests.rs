//! The wider search, against a stand-in for the language model.

use crate::library_tests::{epub, upload};
use crate::testutil::{add_user, send, test_app};
use axum::http::{Method, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// A chat-completions endpoint that answers every query with the same terms
/// and keeps what it was sent.
async fn model(terms: Value) -> (String, Arc<Mutex<Vec<(String, Value)>>>) {
    let seen: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
    let log = seen.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
            let (log, terms) = (log.clone(), terms.clone());
            async move {
                // Like a provider without a JSON mode: it refuses the request
                // that asks for one, and wraps its answer in a code block.
                if body.get("response_format").is_some() {
                    let refusal = json!({ "error": { "message": "response_format is not supported" } });
                    return (StatusCode::BAD_REQUEST, Json(refusal));
                }
                let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                log.lock().unwrap().push((auth, body));
                let content = format!("```json\n{}\n```", json!({ "terms": terms }));
                let answer = json!({
                    "choices": [{ "message": { "role": "assistant", "content": content } }],
                    "usage": { "prompt_tokens": 100, "completion_tokens": 40 },
                });
                (StatusCode::OK, Json(answer))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}

#[tokio::test]
async fn a_query_is_widened_by_the_model() {
    let (_, db, mut state) = test_app().await;
    let (url, seen) = model(json!(["communism", "marx", "sovjet"])).await;
    let mut settings = crate::settings::Settings::default();
    settings.ai = Some(crate::settings::Ai { base_url: url, key: "secret-key".into(), model: "test-model".into() });
    state.settings = Arc::new(settings);
    let app = crate::router(state.clone());
    let (_, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    upload(
        &app,
        &a,
        &[
            ("1.epub", epub("Fully Automated Luxury Communism", "urn:uuid:1")),
            ("2.epub", epub("Karl Marx and the communism of his day", "urn:uuid:2")),
            ("3.epub", epub("Sommarboken", "urn:uuid:3")),
        ],
        "",
    )
    .await;

    // The ordinary search finds nothing for the Swedish word.
    let (_, plain) = send(&app, Method::GET, "/api/search?q=kommunism", Some(&a), None).await;
    assert_eq!(plain["mine"].as_array().unwrap().len(), 0);
    assert!(plain.get("terms").is_none());
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["wider_search"], true);

    // The wider one searches for the query and the model's terms; the book
    // that matches two of them comes first.
    let (status, wide) = send(&app, Method::GET, "/api/search/wider?q=%20Kommunism%20", Some(&a), None).await;
    assert_eq!(status, StatusCode::OK, "{wide}");
    assert_eq!(wide["terms"], json!(["kommunism", "communism", "marx", "sovjet"]));
    let titles: Vec<&str> = wide["mine"].as_array().unwrap().iter().map(|b| b["title"].as_str().unwrap()).collect();
    assert_eq!(titles, ["Karl Marx and the communism of his day", "Fully Automated Luxury Communism"]);
    assert_eq!((wide["usage"]["prompt_tokens"].as_i64(), wide["usage"]["cached"].as_bool()), (Some(100), Some(false)));

    // The model was sent the key and the query, and nothing of the library.
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "Bearer secret-key");
        assert_eq!(seen[0].1["model"], "test-model");
        assert_eq!(seen[0].1["messages"][1]["content"], "kommunism");
        assert!(!seen[0].1.to_string().contains("Luxury"));
    }

    // The same query again, from anyone, is answered without the model.
    let (_, again) = send(&app, Method::GET, "/api/search/wider?q=KOMMUNISM", Some(&b), None).await;
    assert_eq!(again["usage"]["cached"], true);
    assert_eq!(again["mine"].as_array().unwrap().len(), 0, "Bob has no such books");
    assert_eq!(seen.lock().unwrap().len(), 1);

    // The operator sees the use; the key is nowhere in it.
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(&db).await.unwrap();
    let (_, settings) = send(&app, Method::GET, "/api/admin/settings", Some(&a), None).await;
    assert_eq!(settings["wider_search"], json!({ "endpoint": "127.0.0.1", "model": "test-model", "requests": 1, "prompt_tokens": 100, "completion_tokens": 40 }));
    assert!(!settings.to_string().contains("secret-key"));

    // One user cannot run the bill up.
    for _ in 0..60 {
        sqlx::query("INSERT INTO ai_usage (user_id, at, model) SELECT id, ?, 'test-model' FROM users WHERE username = 'bob'")
            .bind(crate::db::now_ts())
            .execute(&db)
            .await
            .unwrap();
    }
    let (status, v) = send(&app, Method::GET, "/api/search/wider?q=something%20new", Some(&b), None).await;
    assert_eq!((status, v["error"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("too-many")));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn without_a_model_there_is_no_wider_search() {
    let (app, db, _) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&a), None).await;
    assert_eq!(config["wider_search"], false);
    let (status, v) = send(&app, Method::GET, "/api/search/wider?q=kommunism", Some(&a), None).await;
    assert_eq!((status, v["error"].as_str()), (StatusCode::NOT_FOUND, Some("off")));
}
