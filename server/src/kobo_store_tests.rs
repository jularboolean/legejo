//! The Kobo store beside Legejo: with the admin setting on, a device keeps
//! its Kobo account. Kobo is played here by a small server of the test's
//! own, which notes what it is asked and answers in Kobo's shapes.

use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const DEVICE: &str = "d9f26b8441cbf1519a6cf89b1a2fbd864dbfbd35e1db395178965909c06683a7";
/// The device's own Kobo account token, which it sends everywhere.
const KOBO_ACCESS: &str = "Bearer kobo-account-token";
/// A book bought in the Kobo store, as the device knows it.
const BOUGHT: &str = "7a1b2c3d-0000-4000-8000-00000000b0b0";
/// One of Legejo's books, and how the device writes its id.
const OURS: &str = "41069f5ea598afe0ec43e1eec0e97766";
const OURS_DASHED: &str = "41069f5e-a598-afe0-ec43-e1eec0e97766";

/// One request as the pretend Kobo received it.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    uri: String,
    headers: HeaderMap,
    body: String,
}

#[derive(Clone, Default)]
struct Kobo {
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Kobo {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn asked(&self, method: &str, uri: &str) -> Option<Seen> {
        self.seen().into_iter().find(|s| s.method == method && s.uri == uri)
    }
}

/// What Kobo answers, by path: just enough of its shapes.
fn kobo_answer(method: &Method, path: &str, headers: &HeaderMap, body: &str) -> (StatusCode, Vec<(&'static str, String)>, String) {
    let ok = |v: Value| (StatusCode::OK, vec![("content-type", "application/json".to_string())], v.to_string());
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match (method.as_str(), segments.as_slice()) {
        ("GET", ["v1", "initialization"]) => ok(json!({ "Resources": {
            "library_sync": "https://storeapi.kobo.com/v1/library/sync",
            "library_items": "https://storeapi.kobo.com/v1/library/items",
            "user_wishlist": "https://storeapi.kobo.com/v1/user/wishlist",
            "device_auth": "https://storeapi.kobo.com/v1/auth/device",
            "image_host": "https://cdn.kobo.com/book-images/",
            "reading_services_host": "https://readingservices.kobo.com",
            "kobo_wishlist_enabled": "True",
        }})),
        ("GET", ["v1", "library", "sync"]) => {
            let token = headers.get("x-kobo-synctoken").and_then(|v| v.to_str().ok()).unwrap_or("");
            let entitlement = |id: &str| json!({ "NewEntitlement": { "BookEntitlement": { "Id": id, "Created": "2026-01-02T03:04:05.1234567Z" } } });
            let (items, next, more) = match token {
                "" => (json!([entitlement(BOUGHT)]), "kobo-1", true),
                "kobo-1" => (json!([entitlement("7a1b2c3d-0000-4000-8000-00000000b0b1")]), "kobo-2", false),
                other => (json!([]), other, false),
            };
            let mut extra = vec![("content-type", "application/json".to_string()), ("x-kobo-synctoken", next.to_string())];
            if more {
                extra.push(("x-kobo-sync", "continue".to_string()));
            }
            (StatusCode::OK, extra, items.to_string())
        }
        ("GET", ["v1", "library", id, "state"]) => ok(json!([{ "EntitlementId": id, "From": "kobo" }])),
        ("PUT", ["v1", "library", _, "state"]) => ok(json!({ "RequestResult": "Success", "Got": serde_json::from_str::<Value>(body).unwrap_or(Value::Null) })),
        ("GET", ["v1", "library", id, "metadata"]) => ok(json!([{ "EntitlementId": id, "Title": "Bought" }])),
        ("DELETE", ["v1", "library", _]) => (StatusCode::NO_CONTENT, vec![], String::new()),
        ("PUT", ["v1", "library", "tags", _]) => ok(json!("kobo-tag")),
        ("POST", ["v1", "auth", "device"]) => ok(json!({ "AccessToken": "from-kobo", "RefreshToken": "from-kobo", "TokenType": "Bearer" })),
        ("GET", ["v1", "products", id]) => (StatusCode::OK, vec![("content-type", "application/json".into()), ("etag", "\"p1\"".into())], json!({ "Product": id }).to_string()),
        ("GET", ["api", "v3", "content", id, "annotations"]) => (
            StatusCode::OK,
            vec![("content-type", "application/json".into()), ("etag", "\"kobo-etag\"".into())],
            json!({ "annotations": [{ "id": format!("hl-of-{id}"), "type": "highlight" }], "nextPageOffsetToken": null }).to_string(),
        ),
        ("PATCH", ["api", "v3", "content", _, "annotations"]) => (StatusCode::NO_CONTENT, vec![], String::new()),
        ("POST", ["api", "v3", "content", "checkforchanges"]) => {
            let asked: Vec<Value> = serde_json::from_str(body).unwrap_or_default();
            ok(Value::Array(asked.iter().filter_map(|i| i.get("ContentId").cloned()).collect()))
        }
        ("GET", ["api", "UserStorage", "Metadata"]) => ok(json!({ "continuationToken": null, "metadata": [{ "kept": "at kobo" }] })),
        _ => (StatusCode::NOT_FOUND, vec![], String::new()),
    }
}

/// The pretend Kobo, listening on this machine; its base URL.
async fn start_kobo() -> (Kobo, String) {
    let kobo = Kobo::default();
    let noted = kobo.clone();
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let noted = noted.clone();
        async move {
            let (parts, body) = request.into_parts();
            let body = String::from_utf8(axum::body::to_bytes(body, usize::MAX).await.unwrap().to_vec()).unwrap();
            noted.seen.lock().unwrap().push(Seen {
                method: parts.method.to_string(),
                uri: parts.uri.to_string(),
                headers: parts.headers.clone(),
                body: body.clone(),
            });
            let (status, headers, body) = kobo_answer(&parts.method, parts.uri.path(), &parts.headers, &body);
            let mut response = axum::response::Response::new(Body::from(body));
            *response.status_mut() = status;
            for (name, value) in headers {
                response.headers_mut().insert(name, value.parse().unwrap());
            }
            response
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (kobo, base)
}

/// A base URL where nothing answers: Kobo out of reach.
async fn nowhere() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

struct Setup {
    app: Router,
    db: sqlx::AnyPool,
    cookie: String,
    token: String,
}

/// Alice, an admin with a Kobo token and one book, on a Legejo whose Kobo
/// is at `kobo`; the store beside Legejo and the highlights as given.
async fn setup(kobo: &str, store: bool, highlights: bool) -> Setup {
    let (_, db, mut state) = test_app().await;
    let services = crate::settings::KoboServices { store: kobo.into(), reading: kobo.into(), images: "https://cdn.test/book-images".into() };
    state.settings = Arc::new(crate::settings::Settings { kobo: services, ..crate::settings::Settings::default() });
    let app = crate::router(state);
    let (alice, cookie) = add_user(&db, "alice").await;
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(alice).execute(&db).await.unwrap();
    let body = json!({ "libris_enabled": true, "registration_enabled": false, "kobo_store_enabled": store, "kobo_annotations_enabled": highlights });
    let (status, v) = send(&app, Method::PUT, "/api/admin/settings", Some(&cookie), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["kobo_store_enabled"], store);
    let (status, v) = send(&app, Method::POST, "/api/account/kobo-token", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let token = v["kobo_token"].as_str().unwrap().to_string();
    sqlx::query("INSERT INTO books (uuid, owner_id, title, file_size, cover_mime) VALUES (?, ?, 'Röda rummet', 1, 'image/jpeg')")
        .bind(OURS)
        .bind(alice)
        .execute(&db)
        .await
        .unwrap();
    Setup { app, db, cookie, token }
}

/// A request as the device makes it: its Kobo token and its id, and
/// optionally the sync token it was last handed.
async fn device(app: &Router, method: Method, uri: &str, sync_token: Option<&str>, body: Option<Value>) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, KOBO_ACCESS)
        .header("x-kobo-deviceid", DEVICE)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = sync_token {
        req = req.header("x-kobo-synctoken", token);
    }
    let req = req.body(body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty)).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn ids(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i.pointer("/NewEntitlement/BookEntitlement/Id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

#[tokio::test]
async fn off_by_default_kobo_is_never_asked() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, false, false).await;
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/initialization", s.token), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(v["Resources"]["library_sync"].as_str().unwrap().contains(&s.token));
    assert!(v["Resources"]["user_wishlist"].as_str().unwrap().contains(&s.token), "the store's keys point here, as before");
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/sync", s.token), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&v), vec![OURS_DASHED.to_string()]);
    let (_, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/products/abc", s.token), None, None).await;
    assert_eq!(v, json!({}), "anything else: the empty object");
    let (status, _, _) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/{BOUGHT}/state", s.token), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = device(&s.app, Method::POST, &format!("/api/kobo/{}/v1/auth/device", s.token), None, Some(json!({ "UserKey": "u" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(kobo.seen().is_empty(), "{:?}", kobo.seen());
}

#[tokio::test]
async fn initialization_is_kobos_own_list_with_legejos_library_in_it() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, true).await;
    let (status, headers, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/initialization", s.token), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let r = &v["Resources"];
    // The store, sign-in and the like stay Kobo's.
    assert_eq!(r["user_wishlist"], "https://storeapi.kobo.com/v1/user/wishlist");
    assert_eq!(r["device_auth"], "https://storeapi.kobo.com/v1/auth/device");
    assert_eq!(r["library_items"], "https://storeapi.kobo.com/v1/library/items");
    assert_eq!(r["kobo_wishlist_enabled"], "True");
    // The library, its collections and covers are Legejo's.
    for key in ["library_sync", "library_metadata", "reading_state", "library_book", "tags", "tag_items", "image_url_template"] {
        assert!(r[key].as_str().unwrap().contains(&s.token), "{key}: {}", r[key]);
    }
    // The highlights come here, which passes Kobo's books' on.
    assert_ne!(r["reading_services_host"], "https://readingservices.kobo.com");
    assert_eq!(headers.get("x-kobo-apitoken").unwrap(), "e30=");
    // Kobo was asked with the device's own account, and without Legejo's token in the path.
    let asked = kobo.asked("GET", "/v1/initialization").expect("Kobo was asked");
    assert_eq!(asked.headers.get(header::AUTHORIZATION).unwrap(), KOBO_ACCESS);
}

#[tokio::test]
async fn initialization_without_kobo_is_legejos_own() {
    let s = setup(&nowhere().await, true, false).await;
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/initialization", s.token), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["Resources"]["library_sync"].as_str().unwrap().contains(&s.token));
    assert!(v["Resources"]["user_wishlist"].as_str().unwrap().contains(&s.token));
}

#[tokio::test]
async fn the_library_sync_has_kobos_books_beside_legejos() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, false).await;
    let uri = format!("/api/kobo/{}/v1/library/sync", s.token);

    // First round: Legejo's book and Kobo's, and Kobo has more.
    let (status, headers, v) = device(&s.app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(ids(&v), vec![OURS_DASHED.to_string(), BOUGHT.to_string()]);
    assert_eq!(headers.get("x-kobo-sync").unwrap(), "continue");
    assert!(headers.get("x-legejo-store").is_none(), "internal headers stay inside");
    // Kobo's entries go to the device as Kobo sent them.
    assert_eq!(v[1]["NewEntitlement"]["BookEntitlement"]["Created"], "2026-01-02T03:04:05.1234567Z");
    let first = headers.get("x-kobo-synctoken").unwrap().to_str().unwrap().to_string();
    assert!(kobo.asked("GET", "/v1/library/sync").unwrap().headers.get("x-kobo-synctoken").is_none());

    // The next round hands Kobo its own token back, and goes on from there.
    let (_, headers, v) = device(&s.app, Method::GET, &uri, Some(&first), None).await;
    assert_eq!(ids(&v), vec!["7a1b2c3d-0000-4000-8000-00000000b0b1".to_string()]);
    assert!(headers.get("x-kobo-sync").is_none());
    let second = headers.get("x-kobo-synctoken").unwrap().to_str().unwrap().to_string();
    let tokens: Vec<String> = kobo
        .seen()
        .iter()
        .filter(|s| s.uri == "/v1/library/sync")
        .map(|s| s.headers.get("x-kobo-synctoken").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default())
        .collect();
    assert_eq!(tokens, vec!["".to_string(), "kobo-1".to_string()]);
    let (_, _, v) = device(&s.app, Method::GET, &uri, Some(&second), None).await;
    assert_eq!(v, json!([]));

    // The system log says what came from the store.
    let (_, log) = send(&s.app, Method::GET, "/api/admin/log", Some(&s.cookie), None).await;
    let synced: Vec<&Value> = log["entries"].as_array().unwrap().iter().filter(|e| e["action"] == "kobo.synced").collect();
    assert!(synced.iter().any(|e| e["details"]["store"] == 1), "{synced:?}");
}

#[tokio::test]
async fn a_device_that_synced_with_kobo_goes_on_from_where_it_was() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, false).await;
    // The token the device got from Kobo itself, before it was pointed here.
    let (_, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/sync", s.token), Some("kobo-1"), None).await;
    assert_eq!(ids(&v), vec![OURS_DASHED.to_string(), "7a1b2c3d-0000-4000-8000-00000000b0b1".to_string()]);
    assert_eq!(kobo.asked("GET", "/v1/library/sync").unwrap().headers.get("x-kobo-synctoken").unwrap(), "kobo-1");
}

#[tokio::test]
async fn a_failed_kobo_sync_loses_nothing() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, false).await;
    let uri = format!("/api/kobo/{}/v1/library/sync", s.token);
    let (_, headers, _) = device(&s.app, Method::GET, &uri, None, None).await;
    let first = headers.get("x-kobo-synctoken").unwrap().to_str().unwrap().to_string();

    // Kobo out of reach: Legejo's part as usual, and Kobo's token stays where it was.
    let down = setup(&nowhere().await, true, false).await;
    sqlx::query("UPDATE users SET kobo_token = ? WHERE username = 'alice'").bind(&s.token).execute(&down.db).await.unwrap();
    let down_uri = format!("/api/kobo/{}/v1/library/sync", s.token);
    let (status, headers, v) = device(&down.app, Method::GET, &down_uri, Some(&first), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(ids(&v), vec![OURS_DASHED.to_string()], "Legejo's own part, from its own database");
    let after = headers.get("x-kobo-synctoken").unwrap().to_str().unwrap().to_string();
    let (_, log) = send(&down.app, Method::GET, "/api/admin/log", Some(&down.cookie), None).await;
    assert!(log["entries"].as_array().unwrap().iter().any(|e| e["action"] == "kobo.synced" && e["details"]["store_failed"] == true), "{log}");

    // Back with Kobo: it is asked from the same place as before the failure.
    let (_, _, v) = device(&s.app, Method::GET, &uri, Some(&after), None).await;
    assert_eq!(ids(&v), vec!["7a1b2c3d-0000-4000-8000-00000000b0b1".to_string()]);
    let last = kobo.seen().into_iter().filter(|s| s.uri == "/v1/library/sync").last().unwrap();
    assert_eq!(last.headers.get("x-kobo-synctoken").unwrap(), "kobo-1");
}

#[tokio::test]
async fn requests_about_kobos_books_go_to_kobo_and_legejos_stay_here() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, false).await;
    let t = &s.token;

    // Legejo's book: answered here, Kobo not asked.
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/v1/library/{OURS_DASHED}/state"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v[0]["EntitlementId"], OURS_DASHED);
    assert!(kobo.seen().is_empty(), "{:?}", kobo.seen());

    // A bought book: Kobo's answer, as it was.
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/v1/library/{BOUGHT}/state?x=1"), None, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!([{ "EntitlementId": BOUGHT, "From": "kobo" }])));
    let asked = kobo.asked("GET", &format!("/v1/library/{BOUGHT}/state?x=1")).unwrap();
    assert_eq!(asked.headers.get(header::AUTHORIZATION).unwrap(), KOBO_ACCESS);
    assert!(!asked.uri.contains(t.as_str()));
    let state = json!({ "ReadingStates": [{ "EntitlementId": BOUGHT, "CurrentBookmark": { "ProgressPercent": 42 } }] });
    let (status, _, v) = device(&s.app, Method::PUT, &format!("/api/kobo/{t}/v1/library/{BOUGHT}/state"), Some("legejo-token"), Some(state.clone())).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["Got"], state);
    let put = kobo.asked("PUT", &format!("/v1/library/{BOUGHT}/state")).unwrap();
    assert!(put.headers.get("x-kobo-synctoken").is_none(), "Legejo's sync token is not Kobo's");
    assert_eq!(serde_json::from_str::<Value>(&put.body).unwrap(), state);
    let (_, _, v) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/v1/library/{BOUGHT}/metadata"), None, None).await;
    assert_eq!(v[0]["Title"], "Bought");
    let (status, _, _) = device(&s.app, Method::DELETE, &format!("/api/kobo/{t}/v1/library/{BOUGHT}"), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(kobo.asked("DELETE", &format!("/v1/library/{BOUGHT}")).is_some());
    // A collection made in the store.
    let (_, _, v) = device(&s.app, Method::PUT, &format!("/api/kobo/{t}/v1/library/tags/kobo-tag"), None, Some(json!({ "Name": "Köpta" }))).await;
    assert_eq!(v, json!("kobo-tag"));
    // Sign-in, products and the rest: Kobo's.
    let (_, _, v) = device(&s.app, Method::POST, &format!("/api/kobo/{t}/v1/auth/device"), None, Some(json!({ "UserKey": "u" }))).await;
    assert_eq!(v["AccessToken"], "from-kobo");
    let (status, headers, v) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/v1/products/p1"), None, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({ "Product": "p1" })));
    assert_eq!(headers.get(header::ETAG).unwrap(), "\"p1\"");

    // Covers: Legejo's here, Kobo's from Kobo's image server.
    let (status, headers, _) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/{BOUGHT}/120/160/false/image.jpg"), None, None).await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(headers.get(header::LOCATION).unwrap(), &format!("https://cdn.test/book-images/{BOUGHT}/120/160/false/image.jpg"));
    let (status, _, _) = device(&s.app, Method::GET, &format!("/api/kobo/{t}/{OURS}/120/160/false/image.jpg"), None, None).await;
    assert_ne!(status, StatusCode::TEMPORARY_REDIRECT);

    // Legejo's own collections and books are still Legejo's.
    let (status, _, v) = device(&s.app, Method::POST, &format!("/api/kobo/{t}/v1/library/tags"), None, Some(json!({ "Name": "Hylla", "Items": [] }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let (status, _, _) = device(&s.app, Method::DELETE, &format!("/api/kobo/{t}/v1/library/{OURS_DASHED}"), None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(kobo.asked("DELETE", &format!("/v1/library/{OURS_DASHED}")).is_none());
    assert!(kobo.seen().iter().all(|s| !s.uri.contains("/v1/library/tags") || s.uri.ends_with("kobo-tag")));
}

#[tokio::test]
async fn kobo_out_of_reach_is_an_error_not_an_empty_answer() {
    let s = setup(&nowhere().await, true, true).await;
    let (status, _, _) = device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/{BOUGHT}/state", s.token), None, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    // The device is known from a request with the token.
    device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/sync", s.token), None, None).await;
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/v3/content/{BOUGHT}/annotations?limit=100"), None, None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{v}");
    let (status, _, _) = device(&s.app, Method::PATCH, &format!("/api/v3/content/{BOUGHT}/annotations"), None, Some(json!({ "updatedAnnotations": [] }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    // Told nothing about Kobo's books, the device keeps what it has; the sync goes on.
    let asks = json!([{ "ContentId": BOUGHT, "etag": "W/\"0\"" }]);
    let (status, _, v) = device(&s.app, Method::POST, "/api/v3/content/checkforchanges", None, Some(asks)).await;
    assert_eq!((status, v), (StatusCode::OK, json!([])));
    let (status, _, v) = device(&s.app, Method::GET, "/api/UserStorage/Metadata", None, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({ "continuationToken": null, "metadata": [] })));
}

#[tokio::test]
async fn highlights_of_kobos_books_are_kobos() {
    let (kobo, base) = start_kobo().await;
    let s = setup(&base, true, true).await;
    device(&s.app, Method::GET, &format!("/api/kobo/{}/v1/library/sync", s.token), None, None).await;

    let (status, headers, v) = device(&s.app, Method::GET, &format!("/api/v3/content/{BOUGHT}/annotations?limit=100"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["annotations"][0]["id"], format!("hl-of-{BOUGHT}"));
    assert_eq!(headers.get(header::ETAG).unwrap(), "\"kobo-etag\"");
    let asked = kobo.asked("GET", &format!("/api/v3/content/{BOUGHT}/annotations?limit=100")).unwrap();
    assert_eq!(asked.headers.get(header::AUTHORIZATION).unwrap(), KOBO_ACCESS);
    let patch = json!({ "updatedAnnotations": [{ "id": "k", "type": "highlight", "highlightedText": "x" }] });
    let (status, _, _) = device(&s.app, Method::PATCH, &format!("/api/v3/content/{BOUGHT}/annotations"), None, Some(patch.clone())).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let patched = kobo.asked("PATCH", &format!("/api/v3/content/{BOUGHT}/annotations")).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&patched.body).unwrap(), patch);
    // Under the token as well, without it at Kobo.
    let (status, _, _) = device(&s.app, Method::GET, &format!("/api/kobo/{}/api/v3/content/{BOUGHT}/annotations", s.token), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(kobo.asked("GET", &format!("/api/v3/content/{BOUGHT}/annotations")).is_some());

    // Asked which books have changed: Kobo answers for its own, Legejo for its.
    let asks = json!([{ "ContentId": OURS_DASHED, "etag": "W/\"0\"" }, { "ContentId": BOUGHT, "etag": "W/\"0\"" }]);
    let (status, _, v) = device(&s.app, Method::POST, "/api/v3/content/checkforchanges", None, Some(asks)).await;
    assert_eq!((status, v), (StatusCode::OK, json!([BOUGHT])));
    let asked = kobo.asked("POST", "/api/v3/content/checkforchanges").unwrap();
    assert_eq!(serde_json::from_str::<Value>(&asked.body).unwrap(), json!([{ "ContentId": BOUGHT, "etag": "W/\"0\"" }]));
    // The rest of the reading services: Kobo's.
    let (_, _, v) = device(&s.app, Method::GET, "/api/UserStorage/Metadata", None, None).await;
    assert_eq!(v["metadata"][0]["kept"], "at kobo");
    // Legejo's own book never goes to Kobo.
    let (status, _, v) = device(&s.app, Method::GET, &format!("/api/v3/content/{OURS_DASHED}/annotations?limit=100"), None, None).await;
    assert_eq!((status, v), (StatusCode::OK, json!({ "annotations": [], "nextPageOffsetToken": null })));
    assert!(kobo.seen().iter().all(|s| !s.uri.contains(OURS_DASHED)));
}
