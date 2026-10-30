//! The public list of Legejo instances, for those who ask to be on it. Off
//! until an admin turns it on. The instance sends the list its own address
//! and nothing else; the list then reads /api/directory here, so what it
//! shows is what the instance says, and turning the setting off answers 404
//! and takes the instance off it.

use crate::admin::setting_bool;
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::time::Duration;

const ANNOUNCE_URL: &str = "https://instances.legejo.se/announce";
const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn listed(state: &AppState) -> bool {
    setting_bool(state, "directory_listed", false).await.unwrap_or(false)
}

/// The address to list: only an `https` public address will do.
pub fn address(state: &AppState) -> Option<String> {
    state.live.public_url().filter(|url| url.starts_with("https://"))
}

/// GET /api/directory: all that the list is told, to anyone who asks.
pub async fn instance(State(state): State<AppState>) -> Response {
    if !listed(&state).await {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(json!({
        "software": "legejo",
        "version": env!("CARGO_PKG_VERSION"),
        "federation": crate::fed::active(&state).await.is_some(),
        "audiobooks": crate::audiobooks::enabled(&state).await,
    }))
    .into_response()
}

/// Tell the list where this instance is. It is asked again at every start,
/// so a list that was away when the setting was turned on still hears of it.
pub fn announce(state: &AppState) {
    let Some(url) = address(state).filter(|_| !cfg!(test)) else {
        return;
    };
    let http = state.http.clone();
    tokio::spawn(async move {
        let sent = http.post(ANNOUNCE_URL).json(&json!({ "url": url })).timeout(ANNOUNCE_TIMEOUT).send().await;
        match sent.map(|r| r.status()) {
            Ok(status) if status.is_success() => tracing::info!("directory: announced {url}"),
            Ok(status) => tracing::warn!("directory: announcing {url}: HTTP {status}"),
            Err(e) => tracing::warn!("directory: announcing {url}: {e}"),
        }
    });
}

pub async fn announce_at_start(state: AppState) {
    if listed(&state).await {
        announce(&state);
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};
    use serde_json::json;

    async fn admin(db: &sqlx::AnyPool) -> String {
        let (id, cookie) = add_user(db, "alice").await;
        sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(id).execute(db).await.unwrap();
        cookie
    }

    fn settings(listed: bool) -> serde_json::Value {
        json!({ "libris_enabled": false, "registration_enabled": false, "directory_listed": listed })
    }

    #[tokio::test]
    async fn an_instance_says_nothing_until_an_admin_asks_to_be_listed() {
        let (_, db, mut state) = test_app().await;
        let admin = admin(&db).await;
        state.live = std::sync::Arc::new(crate::live::Live::fixed(None, None, Some("https://books.example.org".into())));
        let app = crate::router(state);

        let (status, _) = send(&app, Method::GET, "/api/directory", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&admin), Some(settings(true))).await;
        assert_eq!((status, v["directory_listed"].clone()), (StatusCode::OK, json!(true)));
        let (status, v) = send(&app, Method::GET, "/api/directory", None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v, json!({ "software": "legejo", "version": env!("CARGO_PKG_VERSION"), "federation": false, "audiobooks": false }));

        let (status, _) = send(&app, Method::PUT, "/api/admin/settings", Some(&admin), Some(settings(false))).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send(&app, Method::GET, "/api/directory", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_instance_without_an_https_address_cannot_be_listed() {
        let (app, db, _state) = test_app().await;
        let admin = admin(&db).await;
        let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&admin), Some(settings(true))).await;
        assert_eq!((status, v["error"].clone()), (StatusCode::UNPROCESSABLE_ENTITY, json!("public address needed")));
        let (status, _) = send(&app, Method::GET, "/api/directory", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
