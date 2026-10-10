//! Step 0 of syncing highlights from a Kobo: the probe that logs what the
//! device sends to its "reading services".

use crate::settings::Settings;
use crate::testutil::{add_user, send, test_app, test_app_settings};
use axum::http::{Method, StatusCode};
use serde_json::json;

async fn kobo_token(app: &axum::Router, db: &sqlx::AnyPool) -> (String, String) {
    let (_, cookie) = add_user(db, "alice").await;
    let (status, v) = send(app, Method::POST, "/api/account/kobo-token", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    (v["kobo_token"].as_str().unwrap().to_string(), cookie)
}

#[tokio::test]
async fn without_the_flag_the_device_is_not_told_about_us_and_the_probe_is_not_there() {
    let (app, db, _) = test_app().await;
    let (token, _) = kobo_token(&app, &db).await;
    let (status, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["Resources"]["image_host"].is_string());
    assert!(v["Resources"]["readingservices_host"].is_null());
    let body = json!({ "updatedAnnotations": [] });
    for uri in [
        "/api/v3/content/abc/annotations".to_string(),
        format!("/api/kobo/{token}/api/v3/content/abc/annotations"),
        "/api/UserStorage/Metadata".to_string(),
    ] {
        let (status, _) = send(&app, Method::PATCH, &uri, None, Some(body.clone())).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn with_the_flag_the_device_is_pointed_here_and_everything_it_sends_is_answered() {
    let settings = Settings { kobo_annotations_log: true, ..Settings::default() };
    let (app, db, _) = test_app_settings(settings).await;
    let (token, _) = kobo_token(&app, &db).await;
    let (_, v) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/initialization"), None, None).await;
    let host = v["Resources"]["readingservices_host"].as_str().unwrap();
    assert!(host.ends_with(&format!("/api/kobo/{token}")), "{host}");
    assert_eq!(host, v["Resources"]["image_host"].as_str().unwrap());

    let body = json!({ "updatedAnnotations": [{ "id": "x", "type": "highlight", "highlightedText": "Röda rummet" }] });
    for (method, uri) in [
        (Method::PATCH, "/api/v3/content/abc/annotations".to_string()),
        (Method::GET, "/api/v3/content/abc/annotations".to_string()),
        (Method::POST, "/api/v3/content/checkforchanges".to_string()),
        (Method::PATCH, format!("/api/kobo/{token}/api/v3/content/abc/annotations")),
        (Method::GET, format!("/api/kobo/{token}/api/UserStorage/Metadata")),
        (Method::GET, "/api/UserStorage/Metadata".to_string()),
    ] {
        let with_body = matches!(method, Method::PATCH | Method::POST).then(|| body.clone());
        let (status, v) = send(&app, method.clone(), &uri, None, with_body).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri}");
        assert_eq!(v, json!({}), "{method} {uri}");
    }
}
