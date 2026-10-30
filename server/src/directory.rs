//! The public list of Legejo instances, for those who ask to be on it. Off
//! until an admin turns it on. The instance sends the list its own address
//! and nothing else; the list then reads /api/directory here, so what it
//! shows is what the instance says, and turning the setting off answers 404
//! and takes the instance off it.

use crate::admin::setting_bool;
use crate::auth::AdminUser;
use crate::live::{clean_url, instance_id, PublicUrlForm};
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::time::Duration;

const ANNOUNCE_URL: &str = "https://instances.legejo.se/announce";
const ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(15);
const CHECK_URL: &str = "https://instances.legejo.se/check";
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

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
        // Signing up takes a mail to confirm, so without mail no one can.
        "open_signup": state.live.mail().is_some() && crate::register::registration_enabled(&state).await.unwrap_or(false),
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

/// POST /api/admin/connections/public-url/test-outside: have the service try
/// the address from the internet. Only when an admin asks: this is the one
/// thing besides the list that leaves the instance, and it sends the address.
pub async fn test_outside(State(state): State<AppState>, _admin: AdminUser, Json(form): Json<PublicUrlForm>) -> Result<Json<Value>, Response> {
    let Some(url) = clean_url(&form.url)? else {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "url" }))).into_response());
    };
    let ours = instance_id(&state.db).await.map_err(|e| {
        tracing::error!("internal error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;
    let sent = state.http.post(CHECK_URL).json(&json!({ "url": url })).timeout(CHECK_TIMEOUT).send().await;
    let answer = match sent {
        Ok(r) if r.status().is_success() || r.status() == StatusCode::UNPROCESSABLE_ENTITY => r.json::<Value>().await.ok(),
        Ok(r) if r.status() == StatusCode::TOO_MANY_REQUESTS => return Ok(Json(json!({ "ok": false, "error": "too many" }))),
        _ => None,
    };
    Ok(Json(judge(answer, &ours)))
}

/// What the service's answer means here: only this instance's own id proves
/// that it was this instance that was reached.
fn judge(answer: Option<Value>, ours: &str) -> Value {
    let Some(answer) = answer else {
        return json!({ "ok": false, "error": "unavailable" });
    };
    if answer["reachable"] != true && answer["reason"] != "not legejo" {
        return json!({ "ok": false, "error": "unreachable", "reason": answer["reason"] });
    }
    if answer["id"].as_str() != Some(ours) {
        return json!({ "ok": false, "error": "another server" });
    }
    json!({ "ok": true, "version": answer["version"] })
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

    #[test]
    fn a_test_from_outside_counts_only_when_this_instance_answered() {
        let judge = super::judge;
        let reached = |id: &str| Some(json!({ "reachable": true, "id": id, "version": "1.2.3" }));
        assert_eq!(judge(reached("ours"), "ours"), json!({ "ok": true, "version": "1.2.3" }));
        assert_eq!(judge(reached("theirs"), "ours")["error"], "another server");
        let away = json!({ "reachable": false, "reason": "no answer" });
        assert_eq!(judge(Some(away), "ours"), json!({ "ok": false, "error": "unreachable", "reason": "no answer" }));
        assert_eq!(judge(Some(json!({ "reachable": false, "reason": "not legejo" })), "ours")["error"], "another server");
        // A service that is down says nothing about the instance.
        assert_eq!(judge(None, "ours")["error"], "unavailable");
    }

    #[tokio::test]
    async fn the_public_id_tells_the_version() {
        let (app, _, _state) = test_app().await;
        let (status, v) = send(&app, Method::GET, "/api/instance", None, None).await;
        assert_eq!((status, v["version"].clone()), (StatusCode::OK, json!(env!("CARGO_PKG_VERSION"))));
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
        assert_eq!(v, json!({ "software": "legejo", "version": env!("CARGO_PKG_VERSION"), "federation": false, "open_signup": false }));

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
