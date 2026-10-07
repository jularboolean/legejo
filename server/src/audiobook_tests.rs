//! Audiobooks: the setting, files, the podcast feed and partial requests.

use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

/// POST files as multipart/form-data; returns the status and the JSON body.
async fn post_files(app: &Router, uri: &str, cookie: &str, files: &[(&str, &str, &[u8])]) -> (StatusCode, Value) {
    let boundary = "legejo-test-boundary";
    let mut body = Vec::new();
    for (name, mime, bytes) in files {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// GET without a session, as a podcast app does; returns status, headers and body.
async fn fetch(app: &Router, uri: &str, range: Option<&str>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut req = Request::builder().uri(uri).header(header::HOST, "books.test");
    if let Some(range) = range {
        req = req.header(header::RANGE, range);
    }
    let res = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, headers, bytes.to_vec())
}

#[tokio::test]
async fn an_audiobook_is_a_podcast_feed() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let (_, bob) = add_user(&db, "bob").await;

    // Off until an admin turns it on.
    let (status, _) = send(&app, Method::GET, "/api/audiobooks", Some(&alice), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&alice), None).await;
    assert_eq!(config["audiobooks_enabled"], false);
    sqlx::query("INSERT INTO settings (key, value) VALUES ('audiobooks_enabled', 'true')").execute(&db).await.unwrap();
    let (_, config) = send(&app, Method::GET, "/api/config", Some(&alice), None).await;
    assert_eq!(config["audiobooks_enabled"], true);

    let (status, book) = send(&app, Method::POST, "/api/audiobooks", Some(&alice), Some(json!({}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = book["id"].as_i64().unwrap();
    let files_uri = format!("/api/audiobooks/{id}/files");

    // Files are parts in the order they arrive. These are not real audio, so
    // they have no tags and no length; the title comes from the file name.
    let first = b"0123456789".repeat(10);
    let (status, v) = post_files(&app, &files_uri, &alice, &[("sign_of_the_cross_01_64kb.mp3", "audio/mpeg", &first)]).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["title"], "sign of the cross");
    let (_, v) = post_files(&app, &files_uri, &alice, &[("two.m4b", "audio/mp4", b"second"), ("notes.txt", "text/plain", b"x")]).await;
    assert_eq!(v["parts"], 2);
    assert_eq!(v["bytes"], 106);
    assert_eq!(v["errors"].as_array().unwrap().len(), 1, "a text file is left out");
    assert_eq!(v["files"][1]["position"], 2);
    assert!(v.get("feed_key").is_none() && v.get("uuid").is_none());

    let (_, v) = send(&app, Method::PUT, &format!("/api/audiobooks/{id}"), Some(&alice), Some(json!({
        "title": "The Sign of the Cross", "author": "Jean-Joseph Gaume", "narrator": "A <Volunteer>", "language": "en"
    }))).await;
    assert_eq!(v["title"], "The Sign of the Cross");

    // The feed needs no session, only the key in its address.
    let feed_url = v["feed_url"].as_str().unwrap().to_string();
    let path = feed_url[feed_url.find("/podcast/").unwrap()..].to_string();
    assert!(path.starts_with("/podcast/") && path.ends_with("/feed.xml"));
    let (status, headers, body) = fetch(&app, &path, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/rss+xml; charset=utf-8");
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<title>The Sign of the Cross</title>") && xml.contains("<itunes:author>Jean-Joseph Gaume</itunes:author>"));
    assert!(xml.contains("<itunes:type>serial</itunes:type>") && xml.contains("Read by A &lt;Volunteer&gt;."));
    assert_eq!(xml.matches("<item>").count(), 2);
    assert!(xml.contains("<title>Part 1</title>") && xml.contains("<itunes:episode>2</itunes:episode>"));
    assert!(xml.contains("type=\"audio/mpeg\"") && xml.contains("type=\"audio/mp4\"") && xml.contains("length=\"100\""));
    // The first part is the oldest.
    assert!(xml.find("01 Jan 2020").unwrap() < xml.find("02 Jan 2020").unwrap());

    // An episode, whole and in part: podcast apps resume and seek with Range.
    let enclosure = xml.split("enclosure url=\"").nth(1).unwrap().split('"').next().unwrap();
    assert!(enclosure.starts_with("http://books.test/podcast/"), "{enclosure}");
    let audio = enclosure[enclosure.find("/podcast/").unwrap()..].to_string();
    let (status, headers, body) = fetch(&app, &audio, None).await;
    assert_eq!((status, body.len()), (StatusCode::OK, 100));
    assert_eq!(headers[header::ACCEPT_RANGES], "bytes");
    let (status, headers, body) = fetch(&app, &audio, Some("bytes=10-19")).await;
    assert_eq!((status, body.as_slice()), (StatusCode::PARTIAL_CONTENT, b"0123456789".as_slice()));
    assert_eq!(headers[header::CONTENT_RANGE], "bytes 10-19/100");
    let (status, _, body) = fetch(&app, &audio, Some("bytes=-5")).await;
    assert_eq!((status, body.as_slice()), (StatusCode::PARTIAL_CONTENT, b"56789".as_slice()));
    let (status, headers, _) = fetch(&app, &audio, Some("bytes=500-")).await;
    assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(headers[header::CONTENT_RANGE], "bytes */100");

    // Not Bob's, by id; and a wrong key leads nowhere.
    let (status, _) = send(&app, Method::GET, &format!("/api/audiobooks/{id}"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(fetch(&app, "/podcast/0000/feed.xml", None).await.0, StatusCode::NOT_FOUND);
    let (_, list) = send(&app, Method::GET, "/api/audiobooks", Some(&bob), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    // A new key closes the old address at once.
    let (_, v) = send(&app, Method::POST, &format!("/api/audiobooks/{id}/feed-key"), Some(&alice), None).await;
    assert_ne!(v["feed_url"].as_str().unwrap(), feed_url);
    assert_eq!(fetch(&app, &path, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(fetch(&app, &audio, None).await.0, StatusCode::NOT_FOUND);

    // Turned off again: nothing answers, not even a feed that has its key.
    let new_url = v["feed_url"].as_str().unwrap();
    let new_path = new_url[new_url.find("/podcast/").unwrap()..].to_string();
    assert_eq!(fetch(&app, &new_path, None).await.0, StatusCode::OK);
    sqlx::query("UPDATE settings SET value = 'false' WHERE key = 'audiobooks_enabled'").execute(&db).await.unwrap();
    assert_eq!(fetch(&app, &new_path, None).await.0, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE settings SET value = 'true' WHERE key = 'audiobooks_enabled'").execute(&db).await.unwrap();

    // Deleting takes the files along.
    let dir = state.data_dir.join("audio");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/audiobooks/{id}"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}
