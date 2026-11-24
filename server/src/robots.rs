//! What search engines are told. A Legejo is someone's library, not a
//! website: by default every response says `noindex, nofollow`, so neither
//! the sign-in page nor anything else ends up in a search engine. An admin
//! may let the public pages of federated shelves (/f/...) be indexed.
//!
//! The pages stay open to crawlers in robots.txt (the API excepted): a
//! search engine that is not allowed to fetch a page never sees its
//! `noindex`, and keeps an address it already had.

use crate::AppState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

const X_ROBOTS_TAG: HeaderName = HeaderName::from_static("x-robots-tag");
const NOINDEX: &str = "noindex, nofollow";

/// Whether an admin lets search engines index the public shelf pages.
pub async fn enabled(state: &AppState) -> bool {
    state.live.search_engines()
}

/// Paths a search engine may index when the admin allows it.
fn indexable(path: &str) -> bool {
    path.starts_with("/f/")
}

/// Middleware: `noindex, nofollow` on every response but those the admin
/// lets search engines have.
pub async fn tag(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let open = state.live.search_engines() && indexable(request.uri().path());
    let mut response = next.run(request).await;
    if !open {
        response.headers_mut().insert(X_ROBOTS_TAG, HeaderValue::from_static(NOINDEX));
    }
    response
}

/// GET /robots.txt
pub async fn robots_txt(State(state): State<AppState>) -> Response {
    let mut body = String::from("User-agent: *\nDisallow: /api/\nDisallow: /podcast/\nDisallow: /feeds/\n");
    if state.live.search_engines() {
        body.push_str("Allow: /f/\n");
    }
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;

    async fn fetch(app: &axum::Router, uri: &str) -> (StatusCode, Option<String>, String) {
        let res = app.clone().oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let tag = res.headers().get("x-robots-tag").map(|v| v.to_str().unwrap().to_string());
        let body = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        (status, tag, body)
    }

    #[tokio::test]
    async fn search_engines_are_kept_out_unless_an_admin_lets_them_have_the_public_shelves() {
        let (_, db, state) = test_app().await;
        let dir = std::env::temp_dir().join(format!("legejo-web-{}", crate::books::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        // As main() puts it together: the API, then the web, then the tag over all.
        let app = crate::serve_web(crate::router(state.clone()), &dir)
            .layer(axum::middleware::from_fn_with_state(state.clone(), super::tag));
        let noindex = Some("noindex, nofollow".to_string());

        // By default nothing is to be indexed: the sign-in page, the API, a public shelf page.
        assert_eq!(fetch(&app, "/login").await.1, noindex);
        assert_eq!(fetch(&app, "/api/config").await.1, noindex);
        assert_eq!(fetch(&app, "/f/no-such-shelf").await.1, noindex);
        let (status, _, robots) = fetch(&app, "/robots.txt").await;
        assert_eq!(status, StatusCode::OK);
        assert!(robots.contains("Disallow: /api/") && !robots.contains("Allow: /f/"), "{robots}");
        assert!(!robots.contains("Disallow: /\n"), "pages stay fetchable, so that their noindex is read: {robots}");

        // An admin lets search engines have the public shelf pages; only those.
        let (alice, a) = add_user(&db, "alice").await;
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(alice).execute(&db).await.unwrap();
        let body = json!({ "libris_enabled": false, "registration_enabled": false, "search_engines_enabled": true });
        let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&a), Some(body)).await;
        assert_eq!((status, v["search_engines_enabled"].as_bool()), (StatusCode::OK, Some(true)), "{v}");
        assert_eq!(fetch(&app, "/f/no-such-shelf").await.1, None);
        assert_eq!(fetch(&app, "/login").await.1, noindex);
        assert_eq!(fetch(&app, "/api/config").await.1, noindex);
        assert!(fetch(&app, "/robots.txt").await.2.contains("Allow: /f/"));
        // Kept across a restart.
        let live = crate::live::Live::load(&db).await.unwrap();
        assert!(live.search_engines());
    }
}
