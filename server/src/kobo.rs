//! Kobo sync protocol, modelled on the reverse-engineered Kobo Store API as
//! implemented by Calibre-Web. A device is pointed here by setting
//! `api_endpoint=<server>/api/kobo/<token>` in its eReader.conf; the token
//! identifies the user. Unimplemented endpoints return `{}` so the device
//! degrades gracefully instead of failing.

use crate::books::{Book, BOOK_COLUMNS};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Books per sync round; the device asks again while we set `x-kobo-sync: continue`.
const SYNC_BATCH: i64 = 100;

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid sync token" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
}

async fn user_from_token(state: &AppState, token: &str) -> Result<i64, Response> {
    if token.is_empty() {
        return Err(unauthorized());
    }
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE kobo_token = $1")
        .bind(token)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    id.ok_or_else(unauthorized)
}

/// Millisecond timestamps, as elsewhere in the app. The time crate's Rfc3339
/// writes nanoseconds, which the device does not handle reliably.
fn now() -> String {
    crate::db::now_ts()
}

/// Cut every timestamp string in a value down to milliseconds. States and
/// shelves stored before `now()` was fixed carry nine fractional digits.
fn trim_timestamps(value: &mut Value) {
    match value {
        Value::String(s) => {
            if let Some(trimmed) = trimmed_timestamp(s) {
                *s = trimmed;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(trim_timestamps),
        Value::Object(map) => map.values_mut().for_each(trim_timestamps),
        _ => {}
    }
}

/// "2026-10-03T18:56:19.217620743Z" -> "2026-10-03T18:56:19.217Z"; None for
/// anything that is not a UTC timestamp with more than three fractional digits.
fn trimmed_timestamp(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let shaped = b.len() > 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[b.len() - 1] == b'Z'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[20..b.len() - 1].iter().all(u8::is_ascii_digit);
    shaped.then(|| format!("{}Z", &s[..23]))
}

/// A time as the device writes it ("2026-10-12T10:51:24Z", with or without
/// fractions) in the form the database compares: to the millisecond.
fn device_timestamp(s: &str) -> Option<String> {
    let parsed = time::OffsetDateTime::parse(s.trim(), &time::format_description::well_known::Rfc3339).ok()?;
    parsed.to_offset(time::UtcOffset::UTC).format(&crate::db::TS_FORMAT).ok()
}

/// 32-char hex uuid -> dashed 8-4-4-4-12 form the Kobo API uses.
fn dashed(uuid: &str) -> String {
    if uuid.len() == 32 {
        format!("{}-{}-{}-{}-{}", &uuid[0..8], &uuid[8..12], &uuid[12..16], &uuid[16..20], &uuid[20..32])
    } else {
        uuid.to_string()
    }
}

/// External base URL for this token's endpoints. LEGEJO_PUBLIC_URL wins when
/// set: behind a reverse proxy the request may claim http, and every URL
/// handed to the device would then redirect to https, which a PUT or POST
/// does not survive. Otherwise derived from the request (development).
/// Where this server is, with nothing after the host: what a key that names
/// a host alone (readingservices_host) gets. A host with a path in it is
/// not honoured by the device for that key: it sent nothing when tried.
fn origin_url(state: &AppState, headers: &HeaderMap) -> String {
    let token_base = base_url(state, headers, "");
    token_base.trim_end_matches("/api/kobo/").to_string()
}

fn base_url(state: &AppState, headers: &HeaderMap, token: &str) -> String {
    if let Some(config) = &state.fed.config {
        return format!("{}/api/kobo/{token}", config.base);
    }
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:3000");
    format!("{proto}://{host}/api/kobo/{token}")
}

// ---------------------------------------------------------------------------
// Sync token: opaque to the device, carries our incremental-sync watermarks.

#[derive(Serialize, Deserialize, Default)]
struct SyncToken {
    #[serde(default)]
    books_created: String,
    #[serde(default)]
    books_updated: String,
    #[serde(default)]
    state_updated: String,
    #[serde(default)]
    tags_updated: String,
    /// Kobo's own sync token, as Kobo last sent it, when the Kobo store is
    /// kept beside Legejo (kobostore.rs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    kobo: String,
}

impl SyncToken {
    /// Legejo's token as the device hands it back. A token that is not
    /// Legejo's is Kobo's, from a device that synced with Kobo until now:
    /// kept, so that Kobo's sync goes on from where the device is.
    fn from_headers(headers: &HeaderMap) -> Self {
        let Some(raw) = headers.get("x-kobo-synctoken").and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()) else {
            return SyncToken::default();
        };
        let ours = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .filter(|v| v.get("books_created").is_some())
            .and_then(|v| serde_json::from_value(v).ok());
        ours.unwrap_or_else(|| SyncToken { kobo: raw.to_string(), ..SyncToken::default() })
    }

    fn encode(&self) -> String {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(self).unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// Initialization: tells the device where everything lives.

/// The keys that point here also when the rest of the list is Kobo's: the
/// library, its collections and covers. Requests under them about what is
/// not Legejo's are passed on to Kobo (kobostore.rs).
const LEGEJO_RESOURCES: [&str; 13] = [
    "image_host",
    "image_url_template",
    "image_url_quality_template",
    "library_book",
    "library_metadata",
    "library_sync",
    "reading_state",
    "tags",
    "create_tag",
    "delete_tag",
    "rename_tag",
    "tag_items",
    "delete_tag_items",
];

pub async fn initialization(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    user_from_token(&state, &token).await?;
    let base = base_url(&state, &headers, &token);
    let host = base_url(&state, &headers, &token);
    let store = if crate::kobostore::enabled(&state).await { crate::kobostore::resources(&state, &headers).await } else { None };

    let mut resources = json!({
        "device_auth": format!("{base}/v1/auth/device"),
        "device_refresh": format!("{base}/v1/auth/refresh"),
        "image_host": host,
        "image_url_template": format!("{base}/{{ImageId}}/{{width}}/{{height}}/false/image.jpg"),
        "image_url_quality_template": format!("{base}/{{ImageId}}/{{width}}/{{height}}/{{Quality}}/{{isGreyscale}}/image.jpg"),
        "library_book": format!("{base}/v1/library/{{LibraryItemId}}"),
        "library_items": format!("{base}/v1/library/items"),
        "library_metadata": format!("{base}/v1/library/{{Ids}}/metadata"),
        "library_prices": format!("{base}/v1/user/library/previews/prices"),
        "library_search": format!("{base}/v1/library/search"),
        "library_sync": format!("{base}/v1/library/sync"),
        "reading_state": format!("{base}/v1/library/{{Ids}}/state"),
        "tags": format!("{base}/v1/library/tags"),
        "create_tag": format!("{base}/v1/library/tags"),
        "delete_tag": format!("{base}/v1/library/tags/{{TagId}}"),
        "rename_tag": format!("{base}/v1/library/tags/{{TagId}}"),
        "tag_items": format!("{base}/v1/library/tags/{{TagId}}/items"),
        "delete_tag_items": format!("{base}/v1/library/tags/{{TagId}}/items/delete"),
        "user_loyalty_benefits": format!("{base}/v1/user/loyalty/benefits"),
        "user_platform": format!("{base}/v1/user/platform"),
        "user_profile": format!("{base}/v1/user/profile"),
        "user_ratings": format!("{base}/v1/user/ratings"),
        "user_recommendations": format!("{base}/v1/user/recommendations"),
        "user_reviews": format!("{base}/v1/user/reviews"),
        "user_wishlist": format!("{base}/v1/user/wishlist"),
        "post_analytics_event": format!("{base}/v1/analytics/event"),
        "get_tests_request": format!("{base}/v1/analytics/gettests"),
        "book": format!("{base}/v1/products/books/{{ProductId}}"),
        "book_detail_page": format!("{base}/v1/products/books/{{ProductId}}"),
        "product_nextread": format!("{base}/v1/products/{{ProductIds}}/nextread"),
        "product_prices": format!("{base}/v1/products/{{ProductIds}}/prices"),
        "product_recommendations": format!("{base}/v1/products/{{ProductId}}/recommendations"),
        "product_reviews": format!("{base}/v1/products/{{ProductIds}}/reviews"),
        "products": format!("{base}/v1/products"),
        "checkout_borrowed_book": format!("{base}/v1/library/borrow"),
        "configuration_data": format!("{base}/v1/configuration"),
        // Kobo's own sign-in service, as in the stock resource list. Once its
        // account token has expired the device refreshes it here (OpenID
        // discovery) before every sync; without this key it logs "No
        // authorization discovery endpoint found" and the sync fails after
        // initialization.
        "oauth_host": "https://oauth.kobo.com",
        // Kobo's file server, also from the stock list; dictionaries and the
        // user guide are fetched from here.
        "dictionary_host": "https://ereaderfiles.kobo.com",
        "userguide_host": "https://ereaderfiles.kobo.com",
        "store_host": "www.kobo.com",
        "store_newreleases": "https://www.kobo.com/",
        "store_search": "https://www.kobo.com/",
        "store_top50": "https://www.kobo.com/",
        "kobo_audiobooks_enabled": "False",
        "kobo_nativeborrow_enabled": "False",
        "kobo_onestorelibrary_enabled": "False",
        "kobo_redeem_enabled": "False",
        "kobo_shelfie_enabled": "False",
        "kobo_subscriptions_enabled": "False",
        "kobo_superpoints_enabled": "False",
        "kobo_wishlist_enabled": "False",
    });

    // The store beside Legejo: Kobo's own list, with only what concerns
    // Legejo's books pointing here. Sign-in, the store's pages and products
    // stay Kobo's. Without Kobo's list (unreachable), Legejo's alone.
    if let Some(mut kobo) = store {
        for key in LEGEJO_RESOURCES {
            kobo.insert(key.to_string(), resources[key].clone());
        }
        resources = Value::Object(kobo);
    }

    if annotations_enabled(&state).await {
        // The device sends its highlights and notes to its "reading
        // services"; with this key they come here (reading_services).
        // Without the key they go to Kobo as before. The host alone: the
        // device strips any path from this key.
        resources["reading_services_host"] = json!(origin_url(&state, &headers));
    }

    Ok((
        [("x-kobo-apitoken", "e30=")],
        Json(json!({ "Resources": resources })),
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Highlights and notes from the device.
//
// With the admin setting on, `initialization` names this server as the
// device's "reading services" (`reading_services_host`). The device then
// sends its highlights and notes here instead of to Kobo: to the site root,
// without the sync token, naming itself in `x-kobo-deviceid`. That id is
// learnt from whichever token-carrying request names it (`access_log`),
// which is how the user is known.
//
// For each book the device also asks what the server has, and takes the
// answer as the whole truth: whatever is not in it is removed on the device.
// So the answer is what the device itself has sent, in its own shape, with
// the notes as they are here. A device not known yet gets 401, which it
// takes as "later" and keeps what it has; an empty answer would wipe it.

/// Whether an admin has turned the sync of highlights on for the instance.
pub async fn annotations_enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "kobo_annotations_enabled", false).await.unwrap_or(false)
}

fn device_of(headers: &HeaderMap) -> Option<String> {
    let id = headers.get("x-kobo-deviceid")?.to_str().ok()?.trim();
    (!id.is_empty() && id.len() <= 128).then(|| id.to_string())
}

/// The device for the log: the start of its id.
fn device_label(headers: &HeaderMap) -> String {
    device_of(headers).map(|d| d.chars().take(8).collect()).unwrap_or_else(|| "no x-kobo-deviceid".into())
}

/// Note which user a device belongs to, from a request that carries both the
/// token and the device id. Written once an hour per device; a device that
/// is new, or has changed hands, is logged.
async fn remember_device(state: &AppState, user_id: i64, headers: &HeaderMap) {
    let Some(device_id) = device_of(headers) else { return };
    let known: Option<(i64, String)> = sqlx::query_as("SELECT user_id, last_seen_at FROM kobo_devices WHERE device_id = $1")
        .bind(&device_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    if let Some((owner, seen)) = &known {
        if *owner == user_id && *seen > crate::db::ts_in_hours(-1) {
            return;
        }
    }
    let model = headers.get("x-kobo-devicemodel").and_then(|v| v.to_str().ok()).map(|m| m.trim().chars().take(80).collect::<String>());
    let result = sqlx::query(
        "INSERT INTO kobo_devices (device_id, user_id, model, last_seen_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (device_id) DO UPDATE SET user_id = excluded.user_id, model = excluded.model, last_seen_at = excluded.last_seen_at",
    )
    .bind(&device_id)
    .bind(user_id)
    .bind(&model)
    .bind(crate::db::now_ts())
    .execute(&state.db)
    .await;
    match result {
        Ok(_) if known.as_ref().map(|(owner, _)| *owner) != Some(user_id) => {
            tracing::info!("kobo: device {} ({}) belongs to user {user_id}", device_label(headers), model.as_deref().unwrap_or("model unknown"))
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("kobo: could not note device: {e}"),
    }
}

/// The user a device belongs to, when it has synced with its token.
async fn user_of_device(state: &AppState, headers: &HeaderMap) -> Option<i64> {
    let device_id = device_of(headers)?;
    sqlx::query_scalar("SELECT user_id FROM kobo_devices WHERE device_id = $1")
        .bind(&device_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

/// Headers whose values are secrets: logged by length only.
const SECRET_HEADERS: [&str; 4] = ["authorization", "cookie", "x-kobo-userkey", "x-kobo-devicetoken"];

/// The path with the sync token taken out: /api/kobo/<token>/… → /api/kobo/…/….
fn without_token(path: &str) -> String {
    match path.strip_prefix("/api/kobo/") {
        Some(rest) => match rest.split_once('/') {
            Some((_, after)) => format!("/api/kobo/…/{after}"),
            None => "/api/kobo/…".to_string(),
        },
        None => path.to_string(),
    }
}

/// The book id in /api/v3/content/<id>/annotations, as the device writes it (with dashes).
fn content_id(path: &str) -> Option<&str> {
    let rest = path.split("/content/").nth(1)?;
    let id = rest.split('/').next()?;
    (!id.is_empty() && id != "checkforchanges").then_some(id)
}

/// The book whose highlights a request is about, when it is one of those.
fn highlights_of(method: &axum::http::Method, path: &str) -> Option<String> {
    if !path.ends_with("/annotations") || !(method == axum::http::Method::GET || method == axum::http::Method::PATCH) {
        return None;
    }
    content_id(path).map(str::to_string)
}

/// One highlight or note as the device sends it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceAnnotation {
    id: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    highlighted_text: Option<String>,
    note_text: Option<String>,
    /// Where it is in the KEPUB: kept as sent, for placing it later.
    location: Option<Value>,
    chapter_title: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DevicePatch {
    /// Each kept whole as well (`kobo_json`), to be handed back as sent.
    #[serde(default)]
    updated_annotations: Vec<Value>,
    #[serde(default)]
    deleted_annotation_ids: Vec<String>,
}

/// What the device sent for a book, into the owner's annotations. Each is
/// known by the device's own id, so a change comes as an update and not a
/// second row.
async fn take_in(state: &AppState, user_id: i64, book_id: i64, patch: DevicePatch) -> anyhow::Result<(usize, usize)> {
    let now = crate::db::now_ts();
    let mut taken = 0;
    for raw in patch.updated_annotations {
        let a: DeviceAnnotation = match serde_json::from_value(raw.clone()) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("kobo: a highlight in a shape not understood: {e}");
                continue;
            }
        };
        let text: String = a.highlighted_text.as_deref().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
        let note = a.note_text.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        if text.is_empty() && note.is_none() {
            continue;
        }
        let location = a.location.map(|mut l| {
            if let (Some(obj), Some(title)) = (l.as_object_mut(), a.chapter_title.as_deref()) {
                obj.entry("chapterTitle").or_insert_with(|| json!(title));
            }
            l.to_string()
        });
        let kobo_json = raw.to_string();
        let existing: Option<(i64, String, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT id, text, note, kobo_json FROM annotations WHERE user_id = $1 AND kobo_id = $2")
                .bind(user_id)
                .bind(&a.id)
                .fetch_optional(&state.db)
                .await?;
        match existing {
            // The same again (the device hands back what it was served) is
            // no change: the time stays, and the exchange settles.
            Some((id, old_text, old_note, _)) if old_text == text && old_note == note => {
                sqlx::query("UPDATE annotations SET location = COALESCE($1, location), kobo_json = $2 WHERE id = $3")
                    .bind(&location)
                    .bind(&kobo_json)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
            }
            // Dated by the device as before, while it differs here: a copy
            // the device has not yet brought up to date. What is here stands.
            Some((id, _, _, Some(before))) if device_time(&before).is_some() && device_time(&before) == device_time(&kobo_json) => {
                tracing::info!("kobo: highlight {id} came back as it was before; the newer note here stands");
            }
            Some((id, _, _, _)) => {
                sqlx::query("UPDATE annotations SET text = $1, note = $2, location = COALESCE($3, location), kobo_json = $4, updated_at = $5 WHERE id = $6")
                    .bind(&text)
                    .bind(&note)
                    .bind(&location)
                    .bind(&kobo_json)
                    .bind(&now)
                    .bind(id)
                    .execute(&state.db)
                    .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO annotations (book_id, user_id, source, cfi, text, note, kobo_id, location, kobo_json, created_at, updated_at)
                     VALUES ($1, $2, 'kobo', NULL, $3, $4, $5, $6, $7, $8, $8)",
                )
                .bind(book_id)
                .bind(user_id)
                .bind(&text)
                .bind(&note)
                .bind(&a.id)
                .bind(&location)
                .bind(&kobo_json)
                .bind(&now)
                .execute(&state.db)
                .await?;
            }
        }
        let _ = a.kind;
        taken += 1;
    }
    let mut gone = 0;
    for id in patch.deleted_annotation_ids {
        let done = sqlx::query("DELETE FROM annotations WHERE user_id = $1 AND kobo_id = $2")
            .bind(user_id)
            .bind(&id)
            .execute(&state.db)
            .await?;
        gone += done.rows_affected() as usize;
    }
    Ok((taken, gone))
}

/// An annotation as the device takes it back: what it sent, with the note
/// as it is here, dated later when the note has changed here.
fn served(kobo_json: &str, note: Option<&str>, updated_at: &str) -> Option<Value> {
    let mut v: Value = serde_json::from_str(kobo_json).ok()?;
    let obj = v.as_object_mut()?;
    let sent_note = obj.get("noteText").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    if sent_note.as_deref() != note {
        obj.insert("clientLastModifiedUtc".into(), json!(device_dated(updated_at)));
    }
    match note {
        Some(n) => {
            obj.insert("noteText".into(), json!(n));
            obj.insert("type".into(), json!("note"));
        }
        None => {
            obj.remove("noteText");
            if obj.get("type").and_then(Value::as_str) == Some("note") {
                obj.insert("type".into(), json!("highlight"));
            }
        }
    }
    obj.entry("type").or_insert_with(|| json!("highlight"));
    obj.entry("attachments").or_insert_with(|| json!({}));
    obj.entry("context").or_insert(Value::Null);
    Some(v)
}

/// A time of ours as the device writes its own: to the second.
fn device_dated(ts: &str) -> String {
    match ts.split_once('.') {
        Some((whole, _)) => format!("{whole}Z"),
        None => ts.to_string(),
    }
}

/// Highlights made in the web reader, placed in the KEPUB the device reads,
/// so that the device gets them too. Each gets an id of its own and the
/// shape the device uses, kept the way the device's own are (`kobo_json`).
/// One that cannot be found in the KEPUB stays as it is, and is tried again.
async fn place_web_highlights(state: &AppState, user_id: i64, book_id: i64) {
    let waiting: Vec<(i64, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, cfi, text, note, updated_at FROM annotations
         WHERE book_id = $1 AND user_id = $2 AND kobo_json IS NULL AND cfi IS NOT NULL ORDER BY id",
    )
    .bind(book_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    if waiting.is_empty() {
        return;
    }
    let uuid: Option<String> = sqlx::query_scalar("SELECT uuid FROM books WHERE id = $1").bind(book_id).fetch_optional(&state.db).await.ok().flatten();
    let Some(uuid) = uuid else { return };
    let Some(kepub) = ensure_kepub(state, &uuid).await else { return };
    let Ok(bytes) = tokio::fs::read(&kepub).await else { return };
    let bytes = std::sync::Arc::new(bytes);
    let mut placed = 0;
    for (id, cfi, text, note, updated_at) in waiting {
        let (bytes, cfi, text) = (bytes.clone(), cfi, text);
        let found = tokio::task::spawn_blocking(move || crate::kobospan::place(&bytes, &cfi, &text)).await;
        let found = match found {
            Ok(Ok(Some(found))) => found,
            Ok(Ok(None)) => continue,
            Ok(Err(e)) => {
                tracing::warn!("kobo: could not read the kepub of book {book_id} to place highlights: {e:#}");
                return;
            }
            Err(_) => return,
        };
        let kobo_id = dashed(&random_hex());
        let mut a = json!({
            "id": kobo_id,
            "type": if note.is_some() { "note" } else { "highlight" },
            "clientLastModifiedUtc": device_dated(&updated_at),
            "highlightedText": found.text,
            "location": found.location(),
            "attachments": {},
            "context": null,
        });
        if let Some(note) = &note {
            a["noteText"] = json!(note);
        }
        let done = sqlx::query("UPDATE annotations SET kobo_id = $1, kobo_json = $2 WHERE id = $3 AND kobo_json IS NULL")
            .bind(&kobo_id)
            .bind(a.to_string())
            .bind(id)
            .execute(&state.db)
            .await;
        match done {
            Ok(_) => placed += 1,
            Err(e) => tracing::warn!("kobo: could not keep the place of highlight {id}: {e}"),
        }
    }
    if placed > 0 {
        tracing::info!("kobo: {placed} highlights from the web placed in the kepub of book {book_id} for the device");
    }
}

/// When the device last changed a highlight, by its own clock, as it wrote it.
fn device_time(kobo_json: &str) -> Option<String> {
    serde_json::from_str::<Value>(kobo_json).ok()?.get("clientLastModifiedUtc")?.as_str().map(str::to_string)
}

/// The user's book with this uuid, as the device writes it (with dashes).
async fn book_of(state: &AppState, user_id: i64, content: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT id FROM books WHERE uuid = $1 AND owner_id = $2")
        .bind(content.replace('-', ""))
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

/// The answer for a book's highlights: the body, its ETag, and how many.
async fn highlights_body(state: &AppState, user_id: i64, book_id: i64) -> Result<(String, String, usize), sqlx::Error> {
    let rows: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT kobo_json, note, updated_at FROM annotations WHERE book_id = $1 AND user_id = $2 AND kobo_json IS NOT NULL ORDER BY id")
            .bind(book_id)
            .bind(user_id)
            .fetch_all(&state.db)
            .await?;
    let list: Vec<Value> = rows.iter().filter_map(|(j, note, at)| served(j, note.as_deref(), at)).collect();
    let count = list.len();
    let body = json!({ "annotations": list, "nextPageOffsetToken": null }).to_string();
    let digest = <sha2::Sha256 as sha2::Digest>::digest(body.as_bytes());
    let etag = format!("\"{}\"", digest.iter().map(|b| format!("{b:02x}")).collect::<String>());
    Ok((body, etag, count))
}

/// The book's highlights and notes that came from a device, for the device,
/// which is noted as having been handed this answer.
async fn serve_highlights(state: &AppState, device_id: &str, user_id: i64, book_id: i64) -> Response {
    place_web_highlights(state, user_id, book_id).await;
    let (body, etag, _) = match highlights_body(state, user_id, book_id).await {
        Ok(answer) => answer,
        Err(e) => {
            tracing::warn!("kobo: could not read the highlights of book {book_id}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let noted = sqlx::query(
        "INSERT INTO kobo_served (device_id, book_id, etag, served_at) VALUES ($1, $2, $3, $4)
         ON CONFLICT (device_id, book_id) DO UPDATE SET etag = excluded.etag, served_at = excluded.served_at",
    )
    .bind(device_id)
    .bind(book_id)
    .bind(&etag)
    .bind(crate::db::now_ts())
    .execute(&state.db)
    .await;
    if let Err(e) = noted {
        tracing::warn!("kobo: could not note what was handed to the device for book {book_id}: {e}");
    }
    ([(header::CONTENT_TYPE, "application/json".to_string()), (header::ETAG, etag)], body).into_response()
}

/// POST …/checkforchanges: of the books the device asks about, those whose
/// highlights here differ from what this device was last handed. The device
/// fetches those again and takes the changes; told nothing, it keeps its own
/// copy and may hand that back.
///
/// With the store beside Legejo (`store`: the device's headers and the path
/// at Kobo), the books that are not Legejo's are asked about at Kobo, and
/// its answer is added. Kobo out of reach adds nothing: told nothing, the
/// device keeps what it has.
async fn changed_books(state: &AppState, device_id: &str, user_id: i64, body: &[u8], store: Option<(&HeaderMap, &str)>) -> Response {
    let items: Vec<Value> = serde_json::from_slice::<Value>(body).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let asked: Vec<(String, &Value)> = items
        .iter()
        .filter_map(|i| i.as_str().or_else(|| i.get("ContentId").and_then(Value::as_str)).map(|c| (c.to_string(), i)))
        .collect();
    let mut changed = Vec::new();
    let mut kobos = Vec::new();
    for (content, item) in asked.iter().take(1000) {
        let Some(book_id) = book_of(state, user_id, content).await else {
            kobos.push((*item).clone());
            continue;
        };
        place_web_highlights(state, user_id, book_id).await;
        let Ok((_, etag, count)) = highlights_body(state, user_id, book_id).await else { continue };
        let handed: Option<String> = sqlx::query_scalar("SELECT etag FROM kobo_served WHERE device_id = $1 AND book_id = $2")
            .bind(device_id)
            .bind(book_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        let differs = match handed {
            Some(handed) => handed != etag,
            None => count > 0,
        };
        if differs {
            changed.push(content.clone());
        }
    }
    if !changed.is_empty() {
        tracing::info!("kobo: {} of {} books have changed highlights for user {user_id}", changed.len(), asked.len());
    }
    if let (Some((headers, upstream)), false) = (store, kobos.is_empty()) {
        let body = axum::body::Bytes::from(Value::Array(kobos).to_string());
        let answer = crate::kobostore::forward(&state.settings.kobo.reading, &axum::http::Method::POST, upstream, headers, body, None).await;
        match answer {
            Ok(answer) if answer.status.is_success() => match serde_json::from_slice::<Vec<Value>>(&answer.body) {
                Ok(theirs) => changed.extend(theirs.iter().filter_map(|c| c.as_str().map(str::to_string))),
                Err(e) => tracing::warn!("kobo store: changed highlights in a shape not understood: {e}"),
            },
            Ok(answer) => tracing::warn!("kobo store: changed highlights answered HTTP {}", answer.status.as_u16()),
            Err(e) => tracing::warn!("kobo store: changed highlights could not be asked for: {e:#}"),
        }
    }
    Json(changed).into_response()
}

/// Everything a Kobo sends to its "reading services" once they point here.
/// Highlights and notes (PATCH) are taken in and handed back (GET); the rest
/// is answered with the empty shape each call expects, since an error makes
/// the device abort the whole sync. Reached at the site root and under the
/// token. With the admin setting off: 401 for the highlights (the device
/// keeps them) and 404 for the rest.
pub async fn reading_services(
    State(state): State<AppState>,
    method: axum::http::Method,
    original: axum::extract::OriginalUri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let path = without_token(original.0.path());
    let highlights = highlights_of(&method, &path);
    if !annotations_enabled(&state).await {
        return match highlights {
            Some(_) => StatusCode::UNAUTHORIZED.into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    let shown: Vec<String> = headers
        .iter()
        .map(|(name, value)| {
            let name = name.as_str();
            if SECRET_HEADERS.contains(&name) {
                format!("{name}=<{} bytes>", value.len())
            } else {
                format!("{name}={}", value.to_str().unwrap_or("<binary>"))
            }
        })
        .collect();
    tracing::debug!(
        target: "legejo::kobo_annotations",
        "kobo reading services: {method} {path} headers=[{}] body({} bytes)={}",
        shown.join(" "),
        body.len(),
        String::from_utf8_lossy(&body[..body.len().min(64 * 1024)])
    );

    let device = device_of(&headers);
    let user = user_of_device(&state, &headers).await;
    let store = crate::kobostore::enabled(&state).await;
    // The path and query as Kobo's reading services know them: without the
    // token, when the request came under it.
    let upstream = {
        let full = original.0.path();
        let path = match full.strip_prefix("/api/kobo/").and_then(|rest| rest.split_once('/')) {
            Some((_, after)) => format!("/{after}"),
            None => full.to_string(),
        };
        match original.0.query() {
            Some(query) => format!("{path}?{query}"),
            None => path,
        }
    };
    if let Some(content) = highlights {
        let (Some(device_id), Some(user_id)) = (device, user) else {
            tracing::warn!("kobo: highlights {method} from a device that has not synced here ({}): told to come back", device_label(&headers));
            return StatusCode::UNAUTHORIZED.into_response();
        };
        let Some(book_id) = book_of(&state, user_id, &content).await else {
            // A book from the Kobo store: its highlights are Kobo's. Kobo
            // out of reach is an error, never "none", which would wipe them.
            if store {
                return match crate::kobostore::forward(&state.settings.kobo.reading, &method, &upstream, &headers, body, None).await {
                    Ok(answer) => {
                        tracing::info!("kobo store: highlights {method} for {content}: {}", answer.status.as_u16());
                        answer.into_response()
                    }
                    Err(e) => {
                        tracing::warn!("kobo store: highlights {method} for {content} could not be passed on: {e:#}");
                        StatusCode::BAD_GATEWAY.into_response()
                    }
                };
            }
            tracing::warn!("kobo: highlights for a book the device's user does not have ({content})");
            return StatusCode::NOT_FOUND.into_response();
        };
        if method == axum::http::Method::PATCH {
            let patch: DevicePatch = match serde_json::from_slice(&body) {
                Ok(patch) => patch,
                Err(e) => {
                    tracing::warn!("kobo: highlights in a shape not understood: {e}");
                    return StatusCode::BAD_REQUEST.into_response();
                }
            };
            return match take_in(&state, user_id, book_id, patch).await {
                Ok((taken, gone)) => {
                    tracing::info!("kobo: {taken} highlights taken in and {gone} removed for book {book_id} of user {user_id}");
                    StatusCode::NO_CONTENT.into_response()
                }
                Err(e) => {
                    tracing::warn!("kobo: could not take in highlights for book {book_id}: {e:#}");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            };
        }
        return serve_highlights(&state, &device_id, user_id, book_id).await;
    }
    if path.ends_with("/content/checkforchanges") && method == axum::http::Method::POST {
        return match (device, user) {
            (Some(device_id), Some(user_id)) => changed_books(&state, &device_id, user_id, &body, store.then_some((&headers, upstream.as_str()))).await,
            _ => Json(json!([])).into_response(),
        };
    }

    // The rest is Kobo's, with the store beside Legejo; Kobo out of reach
    // gets the empty answer, as without the store.
    if store {
        match crate::kobostore::forward(&state.settings.kobo.reading, &method, &upstream, &headers, body, None).await {
            Ok(answer) => return answer.into_response(),
            Err(e) => tracing::warn!("kobo store: {method} {path} could not be passed on: {e:#}"),
        }
    }

    // What a device takes as "nothing here" on each of its other paths.
    let empty = if path.ends_with("/content/checkforchanges") {
        json!([])
    } else if path.ends_with("/UserStorage/Metadata") {
        json!({ "continuationToken": null, "metadata": [] })
    } else if path.contains("/internal/notebooks") {
        json!({ "data": [], "totalResults": 0 })
    } else {
        json!({})
    };
    (StatusCode::OK, Json(empty)).into_response()
}

// ---------------------------------------------------------------------------
// Device auth: we don't validate anything, just hand back opaque tokens.

fn random_hex() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn auth_device(
    State(state): State<AppState>,
    Path(token): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, Response> {
    user_from_token(&state, &token).await?;
    let user_key = body
        .as_ref()
        .and_then(|b| b.0.get("UserKey"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Json(json!({
        "AccessToken": random_hex(),
        "RefreshToken": random_hex(),
        "TokenType": "Bearer",
        "TrackingId": dashed(&random_hex()),
        "UserKey": user_key,
    })))
}

// ---------------------------------------------------------------------------
// Library sync.

/// Offer a single format. Given both, the device picks the plain EPUB, but a
/// synced book is always opened by the kepub reader, which can only place
/// bookmarks on kepub span markers. In a plain EPUB every position is then
/// reported as the first span of the chapter and the book reopens at the
/// chapter start. Calibre-Web likewise offers only the kepub when it has one.
///
/// A comic is always offered as kepub: it is made into a book with a fixed
/// layout here, without the converter that EPUB books need.
fn download_urls(book: &Book, base: &str, kepub: bool) -> Vec<Value> {
    let (format, extension) = if kepub || book.format == "cbz" { ("KEPUB", "kepub.epub") } else { ("EPUB3", "epub") };
    vec![json!({
        "DrmType": "None",
        "Format": format,
        "Platform": "Generic",
        "Size": book.file_size,
        "Url": format!("{base}/v1/download/{}.{extension}", book.uuid),
    })]
}

/// Stable per-name series id: Kobo firmware groups series by Id.
fn series_id(name: &str) -> String {
    fn fnv1a(seed: u64, data: &str) -> u64 {
        let mut hash = seed;
        for b in data.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
    let a = fnv1a(0xcbf29ce484222325, name);
    let b = fnv1a(0x9e3779b97f4a7c15, name);
    let hex: String = a.to_be_bytes().iter().chain(b.to_be_bytes().iter()).map(|x| format!("{x:02x}")).collect();
    dashed(&hex)
}

fn series_number(index: f64) -> String {
    if index.fract() == 0.0 {
        format!("{}", index as i64)
    } else {
        format!("{index}")
    }
}

fn book_metadata(book: &Book, base: &str, kepub: bool) -> Value {
    let uuid = dashed(&book.uuid);
    let last_modified = book.updated_at.as_deref().unwrap_or(&book.created_at);
    let mut metadata = json!({
        "Categories": ["00000000-0000-0000-0000-000000000001"],
        "ContributorRoles": book.author.as_ref().map(|a| vec![json!({ "Name": a })]).unwrap_or_default(),
        "Contributors": book.author.as_ref().map(|a| vec![a.clone()]).unwrap_or_default(),
        "CoverImageId": book.uuid,
        "CrossRevisionId": uuid,
        "CurrentDisplayPrice": { "CurrencyCode": "USD", "TotalAmount": 0 },
        "CurrentLoveDisplayPrice": { "TotalAmount": 0 },
        "Description": book.description,
        "DownloadUrls": download_urls(book, base, kepub),
        "EntitlementId": uuid,
        "ExternalIds": [],
        "Genre": "00000000-0000-0000-0000-000000000001",
        "IsEligibleForKoboLove": false,
        "IsInternetArchive": false,
        "IsPreOrder": false,
        "IsSocialEnabled": true,
        "Language": book.language.as_deref().unwrap_or("en"),
        "LastModified": last_modified,
        "PhoneticPronunciations": {},
        "PublicationDate": book.published,
        "Publisher": { "Imprint": "", "Name": book.publisher.as_deref().unwrap_or("Unknown") },
        "RevisionId": uuid,
        "Title": book.title,
        "WorkId": uuid,
    });
    if let Some(series) = &book.series {
        let index = book.series_index.unwrap_or(1.0);
        metadata["Series"] = json!({
            "Name": series,
            "Number": series_number(index),
            "NumberFloat": index,
            "Id": series_id(series),
        });
    }
    metadata
}

/// The minutes a stored reading state says the device spent reading, kept
/// when a state is set here so the device's count is not reset.
pub(crate) fn spent_minutes(stored: Option<&str>) -> Option<Value> {
    let state: Value = serde_json::from_str(stored?).ok()?;
    state.pointer("/Statistics/SpentReadingMinutes").filter(|v| v.is_number()).cloned()
}

/// A reading state set here rather than on the device: finished, or back
/// to unread. Every part is dated now, so that the device takes it over its
/// own at the next sync.
pub(crate) fn reading_state_set_here(uuid: &str, created_at: &str, finished: bool) -> Value {
    let now = now();
    let mut status = json!({
        "LastModified": now,
        "Status": if finished { "Finished" } else { "ReadyToRead" },
        "TimesStartedReading": if finished { 1 } else { 0 },
    });
    if finished {
        status["LastTimeFinished"] = json!(now);
    }
    json!({
        "EntitlementId": dashed(uuid),
        "Created": created_at,
        "LastModified": now,
        "PriorityTimestamp": now,
        "StatusInfo": status,
        "Statistics": { "LastModified": now },
        "CurrentBookmark": { "LastModified": now },
    })
}

fn default_reading_state(book: &Book) -> Value {
    let last_modified = book.updated_at.as_deref().unwrap_or(&book.created_at);
    json!({
        "EntitlementId": dashed(&book.uuid),
        "Created": book.created_at,
        "LastModified": last_modified,
        "PriorityTimestamp": last_modified,
        "StatusInfo": {
            "LastModified": last_modified,
            "Status": "ReadyToRead",
            "TimesStartedReading": 0,
        },
        "Statistics": { "LastModified": last_modified },
        "CurrentBookmark": { "LastModified": last_modified },
    })
}

/// The device's own last word on a book, and when it was received.
async fn stored_reading_state(state: &AppState, user_id: i64, book_id: i64) -> Result<Option<(Value, String)>, Response> {
    let raw: Option<(String, String)> =
        sqlx::query_as("SELECT state, updated_at FROM kobo_reading_state WHERE user_id = $1 AND book_id = $2")
            .bind(user_id)
            .bind(book_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    Ok(raw.and_then(|(s, at)| serde_json::from_str::<Value>(&s).ok().filter(Value::is_object).map(|mut state| {
        trim_timestamps(&mut state);
        (state, at)
    })))
}

/// The reading state the device gets for a book: its own last word, unless
/// the book was read further in the web since, in which case the device is
/// pointed to that place in its KEPUB. Without such a place (no text, or
/// text the KEPUB does not have), the device's own state stands, or a fresh
/// one for a book it has not opened.
async fn reading_state_for(state: &AppState, user_id: i64, book: &Book) -> Result<Value, Response> {
    let stored = stored_reading_state(state, user_id, book.id).await?;
    let web: Option<(String, Option<String>, f64, String, Option<String>)> = sqlx::query_as(
        "SELECT cfi, text, percent, updated_at, kobo_json FROM reading_progress WHERE user_id = $1 AND book_id = $2",
    )
    .bind(user_id)
    .bind(book.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if let Some((cfi, Some(text), percent, at, placed)) = web {
        let newer = stored.as_ref().map_or(true, |(_, device)| at.as_str() > device.as_str());
        if newer && book.format == "epub" {
            if let Some(bookmark) = web_bookmark(state, user_id, book, &cfi, &text, percent, &at, placed).await {
                let base = stored.map(|(s, _)| s).unwrap_or_else(|| default_reading_state(book));
                return Ok(reading_state_read_in_web(base, &at, bookmark));
            }
        }
    }
    Ok(stored.map(|(s, _)| s).unwrap_or_else(|| default_reading_state(book)))
}

/// The device's bookmark for the web's position: the span its first words
/// are in, found in the KEPUB and kept in the row until the position
/// changes. None when there is no KEPUB or the text is not in it.
#[allow(clippy::too_many_arguments)]
async fn web_bookmark(
    state: &AppState,
    user_id: i64,
    book: &Book,
    cfi: &str,
    text: &str,
    percent: f64,
    at: &str,
    placed: Option<String>,
) -> Option<Value> {
    if let Some(cached) = placed.and_then(|p| serde_json::from_str::<Value>(&p).ok()) {
        return Some(cached);
    }
    let kepub = ensure_kepub(state, &book.uuid).await?;
    let bytes = std::sync::Arc::new(tokio::fs::read(&kepub).await.ok()?);
    let (cfi, text) = (cfi.to_string(), text.to_string());
    let found = tokio::task::spawn_blocking(move || crate::kobospan::place(&bytes, &cfi, &text)).await;
    let found = match found {
        Ok(Ok(Some(found))) => found,
        Ok(Ok(None)) => {
            tracing::debug!("kobo: the web's place in \"{}\" is not in its kepub", book.title);
            return None;
        }
        Ok(Err(e)) => {
            tracing::warn!("kobo: could not read the kepub of \"{}\" to place the web's position: {e:#}", book.title);
            return None;
        }
        Err(_) => return None,
    };
    let bookmark = json!({
        "ProgressPercent": (percent.clamp(0.0, 1.0) * 100.0).round() as i64,
        "ContentSourceProgressPercent": (found.progress.clamp(0.0, 1.0) * 100.0).round() as i64,
        "Location": { "Value": found.start_id, "Type": "KoboSpan", "Source": found.chapter },
    });
    // Kept for the position it was found for; a newer one has cleared the column.
    let _ = sqlx::query("UPDATE reading_progress SET kobo_json = $1 WHERE user_id = $2 AND book_id = $3 AND updated_at = $4 AND kobo_json IS NULL")
        .bind(bookmark.to_string())
        .bind(user_id)
        .bind(book.id)
        .bind(at)
        .execute(&state.db)
        .await;
    tracing::info!("kobo: the web's place in \"{}\" is {} in {} for the device", book.title, found.start_id, found.chapter);
    Some(bookmark)
}

/// A state pointing the device to a place read in the web: Reading, at that
/// bookmark, every part dated by the web's time so that the device takes it
/// over its own, older one. What the device counted stays.
fn reading_state_read_in_web(mut base: Value, at: &str, mut bookmark: Value) -> Value {
    base["LastModified"] = json!(at);
    base["PriorityTimestamp"] = json!(at);
    let started = base.pointer("/StatusInfo/TimesStartedReading").and_then(Value::as_i64).unwrap_or(0).max(1);
    let first_started = base.pointer("/StatusInfo/LastTimeStartedReading").cloned().unwrap_or_else(|| json!(at));
    base["StatusInfo"] = json!({
        "LastModified": at,
        "Status": "Reading",
        "TimesStartedReading": started,
        "LastTimeStartedReading": first_started,
    });
    if base.get("Statistics").is_none() {
        base["Statistics"] = json!({ "LastModified": at });
    }
    bookmark["LastModified"] = json!(at);
    base["CurrentBookmark"] = bookmark;
    base
}

fn entitlement(book: &Book, reading_state: Value, base: &str, kepub: bool, change_type: &str) -> Value {
    let uuid = dashed(&book.uuid);
    let last_modified = book.updated_at.as_deref().unwrap_or(&book.created_at);
    json!({
        change_type: {
            "BookEntitlement": {
                "Accessibility": "Full",
                "ActivePeriod": { "From": now() },
                "Created": book.created_at,
                "CrossRevisionId": uuid,
                "Id": uuid,
                "IsRemoved": false,
                "IsHiddenFromArchive": false,
                "IsLocked": false,
                "LastModified": last_modified,
                "OriginCategory": "Imported",
                "RevisionId": uuid,
                "Status": "Active",
            },
            "BookMetadata": book_metadata(book, base, kepub),
            "ReadingState": reading_state,
        }
    })
}

/// The account behind a token, for the system log.
async fn actor_for_token(state: &AppState, token: &str) -> Option<(i64, String)> {
    sqlx::query_as("SELECT id, username FROM users WHERE kobo_token = $1")
        .bind(token)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

/// One sync round, plus an audit log entry with what was sent or that it
/// failed. This only covers the server side; whether the device then fetches
/// the books shows up as kobo.downloaded entries.
pub async fn library_sync(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let result = sync_round(&state, &token, &headers).await;
    // Extract what the log needs first: a borrowed Response cannot be held
    // across an await.
    let outcome: Result<Value, u16> = match &result {
        Ok(response) => {
            let sent = |kind: &str| -> i64 {
                response
                    .headers()
                    .get(format!("x-legejo-{kind}"))
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0)
            };
            let mut details = json!({ "new": sent("new"), "changed": sent("changed"), "collections": sent("collections"),
                       "states": sent("states"), "more": response.headers().contains_key("x-kobo-sync") });
            if response.headers().contains_key("x-legejo-store") {
                details["store"] = json!(sent("store"));
            }
            if response.headers().contains_key("x-legejo-store-failed") {
                details["store_failed"] = json!(true);
            }
            Ok(details)
        }
        Err(response) => Err(response.status().as_u16()),
    };
    let actor = actor_for_token(&state, &token).await;
    let who = actor.as_ref().map(|(id, name)| (*id, name.as_str()));
    match outcome {
        Ok(details) => crate::audit::log(&state, who, "kobo.synced", details).await,
        Err(status) => {
            tracing::warn!("kobo sync failed for {}: HTTP {status}", who.map(|(_, n)| n).unwrap_or("unknown token"));
            let action = if who.is_some() { "kobo.sync_failed" } else { "kobo.denied" };
            crate::audit::log(&state, who, action, json!({ "status": status })).await;
        }
    }
    result.map(|mut response| {
        for kind in ["new", "changed", "collections", "states", "store", "store-failed"] {
            response.headers_mut().remove(format!("x-legejo-{kind}"));
        }
        response
    })
}

async fn sync_round(state: &AppState, token: &str, headers: &HeaderMap) -> Result<Response, Response> {
    let state = state.clone();
    let token = token.to_string();
    let headers = headers.clone();
    let user_id = user_from_token(&state, &token).await?;
    let base = base_url(&state, &headers, &token);
    let mut sync_token = SyncToken::from_headers(&headers);
    let created_wm = sync_token.books_created.clone();
    let mut items: Vec<Value> = Vec::new();
    let mut continue_sync = false;

    // 1. New books since the last sync, in stable creation order.
    let new_books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS} FROM books
         WHERE owner_id = $1 AND created_at > $2 AND kobo_removed_at IS NULL AND format IN ('epub', 'cbz')
         ORDER BY created_at, id
         LIMIT $3"
    ))
    .bind(user_id)
    .bind(&sync_token.books_created)
    .bind(SYNC_BATCH + 1)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    let new_books = if new_books.len() as i64 > SYNC_BATCH {
        continue_sync = true;
        &new_books[..SYNC_BATCH as usize]
    } else {
        &new_books[..]
    };
    let kepub = state.kepubify.is_some();
    for book in new_books {
        let reading_state = reading_state_for(&state, user_id, book).await?;
        items.push(entitlement(book, reading_state, &base, kepub, "NewEntitlement"));
        sync_token.books_created = book.created_at.clone();
    }

    // 2. Metadata changes for books the device already has.
    if !continue_sync {
        let changed: Vec<Book> = sqlx::query_as(&format!(
            "SELECT {BOOK_COLUMNS} FROM books
             WHERE owner_id = $1 AND updated_at IS NOT NULL AND updated_at > $2
               AND created_at <= $3 AND kobo_removed_at IS NULL AND format IN ('epub', 'cbz')
             ORDER BY updated_at, id
             LIMIT $4"
        ))
        .bind(user_id)
        .bind(&sync_token.books_updated)
        .bind(&created_wm)
        .bind(SYNC_BATCH + 1)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;

        let changed = if changed.len() as i64 > SYNC_BATCH {
            continue_sync = true;
            &changed[..SYNC_BATCH as usize]
        } else {
            &changed[..]
        };
        for book in changed {
            let reading_state = reading_state_for(&state, user_id, book).await?;
            items.push(entitlement(book, reading_state, &base, kepub, "ChangedEntitlement"));
            sync_token.books_updated = book.updated_at.clone().unwrap_or_default();
        }
    }

    // 3. Shelves as Kobo collections (tags), including deletions via tombstones.
    if !continue_sync {
        let tags_wm = sync_token.tags_updated.clone();
        let shelves: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, created_at, updated_at FROM shelves
             WHERE owner_id = $1 AND (created_at > $2 OR COALESCE(updated_at, '') > $3)
             ORDER BY COALESCE(updated_at, created_at)",
        )
        .bind(user_id)
        .bind(&tags_wm)
        .bind(&tags_wm)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        for (shelf_id, name, created_at, updated_at) in shelves {
            let change_type = if created_at > tags_wm { "NewTag" } else { "ChangedTag" };
            items.push(json!({ change_type: {
                "Tag": tag_json(&state, user_id, shelf_id, &name, &created_at, updated_at.as_deref()).await?,
            }}));
            let stamp = updated_at.unwrap_or(created_at);
            if stamp > sync_token.tags_updated {
                sync_token.tags_updated = stamp;
            }
        }

        let deleted: Vec<(i64, String)> = sqlx::query_as(
            "SELECT shelf_id, deleted_at FROM kobo_deleted_shelves
             WHERE owner_id = $1 AND deleted_at > $2
             ORDER BY deleted_at",
        )
        .bind(user_id)
        .bind(&tags_wm)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        for (shelf_id, deleted_at) in deleted {
            items.push(json!({ "DeletedTag": {
                "Tag": { "Id": tag_id(shelf_id), "LastModified": deleted_at },
            }}));
            if deleted_at > sync_token.tags_updated {
                sync_token.tags_updated = deleted_at;
            }
        }
    }

    // 4. Reading-state changes: the device's own (e.g. from another device)
    //    and places read in the web since the last sync, in time order.
    if !continue_sync {
        let mut changed: Vec<(i64, String)> = sqlx::query_as(
            "SELECT book_id, updated_at FROM kobo_reading_state
             WHERE user_id = $1 AND updated_at > $2
             ORDER BY updated_at
             LIMIT 200",
        )
        .bind(user_id)
        .bind(&sync_token.state_updated)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        let read_in_web: Vec<(i64, String)> = sqlx::query_as(
            "SELECT rp.book_id, rp.updated_at FROM reading_progress rp
             JOIN books b ON b.id = rp.book_id
             WHERE rp.user_id = $1 AND rp.updated_at > $2 AND rp.text IS NOT NULL
               AND b.kobo_removed_at IS NULL AND b.format = 'epub'
             ORDER BY rp.updated_at
             LIMIT 200",
        )
        .bind(user_id)
        .bind(&sync_token.state_updated)
        .fetch_all(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        changed.extend(read_in_web);
        changed.sort_by(|a, b| a.1.cmp(&b.1));
        changed.truncate(200);
        let mut sent: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for (book_id, updated_at) in changed {
            if sent.insert(book_id) {
                if let Some(book) = book_by_id(&state, user_id, book_id).await? {
                    let value = reading_state_for(&state, user_id, &book).await?;
                    items.push(json!({ "ChangedReadingState": { "ReadingState": value } }));
                }
            }
            if updated_at > sync_token.state_updated {
                sync_token.state_updated = updated_at;
            }
        }
    }

    items.iter_mut().for_each(trim_timestamps);

    // Per-kind counts for the audit log, passed to library_sync as internal
    // headers that it strips before responding.
    let count = |keys: &[&str]| items.iter().filter(|i| keys.iter().any(|k| i.get(k).is_some())).count();
    let counts = [
        ("new", count(&["NewEntitlement"])),
        ("changed", count(&["ChangedEntitlement"])),
        ("collections", count(&["NewTag", "ChangedTag", "DeletedTag"])),
        ("states", count(&["ChangedReadingState"])),
    ];
    // 5. Kobo's own sync for the same device, when the store is kept beside
    //    Legejo: once Legejo has nothing more this round. Kobo's entries go
    //    to the device as Kobo sent them. A failure leaves Kobo's token as
    //    it was, so that the next round asks Kobo again from there.
    let mut store_headers = Vec::new();
    let mut store_count: Option<usize> = None;
    let mut store_failed = false;
    if !continue_sync && crate::kobostore::enabled(&state).await {
        match crate::kobostore::sync(&state, &headers, &sync_token.kobo).await {
            Ok(store) => {
                store_count = Some(store.items.len());
                items.extend(store.items);
                if let Some(next) = store.token {
                    sync_token.kobo = next;
                }
                store_headers = store.headers;
            }
            Err(e) => {
                tracing::warn!("kobo store: the library sync could not be fetched: {e:#}");
                store_failed = true;
            }
        }
    }

    let mut response = Json(items).into_response();
    if let Some(n) = store_count {
        response.headers_mut().insert("x-legejo-store", axum::http::HeaderValue::from(n as u64));
    }
    if store_failed {
        response.headers_mut().insert("x-legejo-store-failed", axum::http::HeaderValue::from_static("1"));
    }
    for (name, value) in store_headers {
        response.headers_mut().insert(name, value);
    }
    for (kind, n) in counts {
        if let Ok(name) = axum::http::HeaderName::from_bytes(format!("x-legejo-{kind}").as_bytes()) {
            response.headers_mut().insert(name, axum::http::HeaderValue::from(n as u64));
        }
    }
    let header_token = axum::http::HeaderValue::from_str(&sync_token.encode())
        .map_err(|e| internal(e.into()))?;
    response.headers_mut().insert("x-kobo-synctoken", header_token);
    if continue_sync {
        response.headers_mut().insert("x-kobo-sync", axum::http::HeaderValue::from_static("continue"));
    }
    Ok(response)
}

// ---------------------------------------------------------------------------
// Shelves as Kobo collections (tags).

/// Stable collection id derived from the shelf id, parseable back out.
fn tag_id(shelf_id: i64) -> String {
    format!("beefbeef-0000-4000-a000-{shelf_id:012x}")
}

pub(crate) fn parse_tag_id(tag: &str) -> Option<i64> {
    tag.strip_prefix("beefbeef-0000-4000-a000-")
        .and_then(|h| i64::from_str_radix(h, 16).ok())
}

async fn tag_json(
    state: &AppState,
    user_id: i64,
    shelf_id: i64,
    name: &str,
    created_at: &str,
    updated_at: Option<&str>,
) -> Result<Value, Response> {
    let uuids: Vec<String> = sqlx::query_scalar(
        "SELECT b.uuid FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1 AND b.owner_id = $2 AND b.format IN ('epub', 'cbz')",
    )
    .bind(shelf_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let items: Vec<Value> = uuids
        .iter()
        .map(|u| json!({ "RevisionId": dashed(u), "Type": "ProductRevisionTagItem" }))
        .collect();
    Ok(json!({
        "Created": created_at,
        "Id": tag_id(shelf_id),
        "Items": items,
        "LastModified": updated_at.unwrap_or(created_at),
        "Name": name,
        "Type": "UserTag",
    }))
}

/// Resolve a device-supplied tag id to one of the user's shelves.
async fn owned_shelf(state: &AppState, user_id: i64, tag: &str) -> Result<i64, Response> {
    let shelf_id = parse_tag_id(tag).ok_or_else(not_found)?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM shelves WHERE id = $1 AND owner_id = $2")
        .bind(shelf_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    exists.ok_or_else(not_found)
}

fn body_items(body: &Value) -> Vec<String> {
    body.get("Items")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("RevisionId").and_then(|v| v.as_str()))
                .map(|u| u.chars().filter(|c| *c != '-').collect())
                .collect()
        })
        .unwrap_or_default()
}

async fn touch_shelf(state: &AppState, shelf_id: i64) -> Result<(), Response> {
    sqlx::query("UPDATE shelves SET updated_at = $1 WHERE id = $2")
        .bind(now())
        .bind(shelf_id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(())
}

async fn set_shelf_items(
    state: &AppState,
    user_id: i64,
    shelf_id: i64,
    uuids: &[String],
    add: bool,
) -> Result<(), Response> {
    let federated: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM shelves WHERE id = $1 AND visibility = 'federated'",
    )
    .bind(shelf_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?
        > 0;
    for uuid in uuids {
        // The device cannot show an error, so a book that may not federate
        // is silently skipped when added to a federated shelf.
        if add && federated {
            let book: Option<crate::books::Book> = sqlx::query_as(&format!(
                "SELECT {} FROM books WHERE uuid = $1 AND owner_id = $2",
                crate::books::BOOK_COLUMNS
            ))
            .bind(uuid)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
            if let Some(book) = book {
                if let Err(reason) = crate::license::federable(&book.license_facts(), crate::license::current_year()) {
                    tracing::warn!(
                        "kobo: not adding {uuid} to federated shelf {shelf_id}: {}",
                        serde_json::to_string(&reason).unwrap_or_default()
                    );
                    continue;
                }
            }
        }
        let query = if add {
            "INSERT INTO shelf_books (shelf_id, book_id)
             SELECT $1, id FROM books WHERE uuid = $2 AND owner_id = $3
             ON CONFLICT (shelf_id, book_id) DO NOTHING"
        } else {
            "DELETE FROM shelf_books WHERE shelf_id = $1
             AND book_id IN (SELECT id FROM books WHERE uuid = $2 AND owner_id = $3)"
        };
        sqlx::query(query)
            .bind(shelf_id)
            .bind(uuid)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    }
    state.fed.wake.notify_one();
    touch_shelf(state, shelf_id).await
}

/// Device created a collection: make a shelf (reusing one with the same name).
pub async fn create_tag(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let name = body
        .get("Name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing Name" }))).into_response()
        })?;

    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM shelves WHERE owner_id = $1 AND LOWER(name) = LOWER($2)")
            .bind(user_id)
            .bind(name)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    let shelf_id = match existing {
        Some(id) => id,
        None => sqlx::query_scalar("INSERT INTO shelves (owner_id, name) VALUES ($1, $2) RETURNING id")
            .bind(user_id)
            .bind(name)
            .fetch_one(&state.db)
            .await
            .map_err(|e| internal(e.into()))?,
    };
    set_shelf_items(&state, user_id, shelf_id, &body_items(&body), true).await?;
    Ok((StatusCode::CREATED, Json(json!(tag_id(shelf_id)))).into_response())
}

pub async fn rename_tag(
    State(state): State<AppState>,
    Path((token, tag)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let shelf_id = owned_shelf(&state, user_id, &tag).await?;
    let name = body
        .get("Name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing Name" }))).into_response()
        })?;
    sqlx::query("UPDATE shelves SET name = $1, updated_at = $2 WHERE id = $3")
        .bind(name)
        .bind(now())
        .bind(shelf_id)
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(json!(tag_id(shelf_id))))
}

pub async fn delete_tag(
    State(state): State<AppState>,
    Path((token, tag)): Path<(String, String)>,
) -> Result<StatusCode, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let shelf_id = owned_shelf(&state, user_id, &tag).await?;
    sqlx::query("DELETE FROM shelves WHERE id = $1")
        .bind(shelf_id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    sqlx::query(
        "INSERT INTO kobo_deleted_shelves (shelf_id, owner_id, deleted_at) VALUES ($1, $2, $3)
         ON CONFLICT (shelf_id) DO UPDATE SET owner_id = excluded.owner_id, deleted_at = excluded.deleted_at",
    )
    .bind(shelf_id)
    .bind(user_id)
    .bind(now())
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(StatusCode::OK)
}

pub async fn add_tag_items(
    State(state): State<AppState>,
    Path((token, tag)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let shelf_id = owned_shelf(&state, user_id, &tag).await?;
    set_shelf_items(&state, user_id, shelf_id, &body_items(&body), true).await?;
    Ok(StatusCode::CREATED)
}

pub async fn delete_tag_items(
    State(state): State<AppState>,
    Path((token, tag)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<StatusCode, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let shelf_id = owned_shelf(&state, user_id, &tag).await?;
    set_shelf_items(&state, user_id, shelf_id, &body_items(&body), false).await?;
    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Per-book endpoints.

async fn book_by_uuid(state: &AppState, user_id: i64, uuid: &str) -> Result<Book, Response> {
    let clean: String = uuid.chars().filter(|c| *c != '-').collect();
    let book: Option<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS} FROM books WHERE uuid = $1 AND owner_id = $2 AND format IN ('epub', 'cbz')"
    ))
    .bind(&clean)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    book.ok_or_else(not_found)
}

/// The user's book with this id, when it is one the device can have.
async fn book_by_id(state: &AppState, user_id: i64, id: i64) -> Result<Option<Book>, Response> {
    sqlx::query_as(&format!("SELECT {BOOK_COLUMNS} FROM books WHERE id = $1 AND owner_id = $2 AND format IN ('epub', 'cbz')"))
        .bind(id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))
}

pub async fn library_metadata(
    State(state): State<AppState>,
    Path((token, uuid)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let base = base_url(&state, &headers, &token);
    let book = book_by_uuid(&state, user_id, &uuid).await?;
    Ok(Json(json!([book_metadata(&book, &base, state.kepubify.is_some())])))
}

pub async fn reading_state_get(
    State(state): State<AppState>,
    Path((token, uuid)): Path<(String, String)>,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let book = book_by_uuid(&state, user_id, &uuid).await?;
    let reading_state = reading_state_for(&state, user_id, &book).await?;
    Ok(Json(json!([reading_state])))
}

pub async fn reading_state_put(
    State(state): State<AppState>,
    Path((token, uuid)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Response> {
    let user_id = user_from_token(&state, &token).await?;
    let book = book_by_uuid(&state, user_id, &uuid).await?;

    let mut incoming = body
        .get("ReadingStates")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| {
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "missing ReadingStates" })))
                .into_response()
        })?;
    if let Some(obj) = incoming.as_object_mut() {
        obj.insert("EntitlementId".into(), json!(dashed(&book.uuid)));
        obj.insert("LastModified".into(), json!(now()));
    }
    // A state without statistics keeps the ones already counted.
    if incoming.get("Statistics").is_none() {
        if let Some((stored, _)) = stored_reading_state(&state, user_id, book.id).await? {
            if let Some(statistics) = stored.get("Statistics") {
                incoming["Statistics"] = statistics.clone();
            }
        }
    }

    sqlx::query(
        "INSERT INTO kobo_reading_state (user_id, book_id, state, updated_at)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, book_id)
         DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(book.id)
    .bind(incoming.to_string())
    .bind(now())
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;

    // Marked as unread on the device: that is the latest word on the book,
    // so a place read in the web or in KOReader before it goes, and so does
    // a mark set by hand. Anything newer than the device's decision stays.
    let status = incoming.pointer("/StatusInfo/Status").and_then(Value::as_str);
    let decided = incoming.pointer("/StatusInfo/LastModified").and_then(Value::as_str).and_then(device_timestamp);
    if let (Some("ReadyToRead"), Some(since)) = (status, decided) {
        for table in ["reading_progress", "kosync_progress"] {
            let _ = sqlx::query(&format!("DELETE FROM {table} WHERE user_id = $1 AND book_id = $2 AND updated_at < $3"))
                .bind(user_id)
                .bind(book.id)
                .bind(&since)
                .execute(&state.db)
                .await;
        }
        let _ = sqlx::query("UPDATE books SET read_at = NULL WHERE id = $1 AND read_at IS NOT NULL AND read_at < $2")
            .bind(book.id)
            .bind(&since)
            .execute(&state.db)
            .await;
        tracing::info!("kobo: \"{}\" is unread on the device since {since}", book.title);
    }

    Ok(Json(json!({
        "RequestResult": "Success",
        "UpdateResults": [{
            "EntitlementId": dashed(&book.uuid),
            "CurrentBookmarkResult": { "Result": "Success" },
            "StatisticsResult": { "Result": "Success" },
            "StatusInfoResult": { "Result": "Success" },
        }],
    })))
}

/// "Remove from library" on the device. The book stays in the catalog but is
/// marked as removed from the Kobo so later syncs do not bring it back; see
/// books::kobo_restore for sending it again.
pub async fn library_delete(
    State(state): State<AppState>,
    Path((token, uuid)): Path<(String, String)>,
) -> Result<StatusCode, Response> {
    let user_id = user_from_token(&state, &token).await?;
    // The device also deletes books it got elsewhere; ignore those.
    if let Ok(book) = book_by_uuid(&state, user_id, &uuid).await {
        sqlx::query("UPDATE books SET kobo_removed_at = $1 WHERE id = $2")
            .bind(now())
            .bind(book.id)
            .execute(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        let actor = actor_for_token(&state, &token).await;
        let who = actor.as_ref().map(|(id, name)| (*id, name.as_str()));
        crate::audit::log(&state, who, "kobo.removed", json!({ "title": book.title })).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A comic archive as a book with a fixed layout, which is what the device
/// reads; made once and kept beside the converted EPUB books. None when the
/// archive cannot be read.
async fn ensure_comic(state: &AppState, book: &Book) -> Option<std::path::PathBuf> {
    let out_dir = state.data_dir.join("kepub");
    let out = out_dir.join(format!("{}.kepub.epub", book.uuid));
    if tokio::fs::try_exists(&out).await.unwrap_or(false) {
        return Some(out);
    }
    tokio::fs::create_dir_all(&out_dir).await.ok()?;
    let src = crate::books::book_path(state, &book.uuid, crate::formats::Format::Cbz);
    // Written beside its final name and moved there, so that a half-made
    // file is never served and two requests never share one.
    let tmp = out_dir.join(format!("{}.{}.tmp", book.uuid, crate::books::new_uuid()));
    let meta = crate::formats::ComicMeta {
        uuid: book.uuid.clone(),
        title: book.title.clone(),
        author: book.author.clone(),
        language: book.language.clone(),
    };
    let (from, to) = (src, tmp.clone());
    let made = tokio::task::spawn_blocking(move || crate::formats::cbz_to_epub(&from, &to, &meta)).await;
    match made {
        Ok(Ok(_)) => {
            tokio::fs::rename(&tmp, &out).await.ok()?;
            Some(out)
        }
        Ok(Err(e)) => {
            tracing::warn!("comic {} could not be made into a book: {e:#}", book.uuid);
            let _ = tokio::fs::remove_file(&tmp).await;
            None
        }
        Err(e) => {
            tracing::warn!("comic conversion did not finish: {e}");
            let _ = tokio::fs::remove_file(&tmp).await;
            None
        }
    }
}

/// At most this many conversions at once: a device fetching a whole library
/// must not start one kepubify per book.
static KEPUB_GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
/// Longer than this and the conversion is given up (and the process ended).
const KEPUB_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Convert to kepub with the external `kepubify` binary, caching the result.
/// Returns None (fall back to plain epub) when conversion is unavailable or fails.
async fn ensure_kepub(state: &AppState, uuid: &str) -> Option<std::path::PathBuf> {
    let out_dir = state.data_dir.join("kepub");
    let out = out_dir.join(format!("{uuid}.kepub.epub"));
    if tokio::fs::try_exists(&out).await.unwrap_or(false) {
        return Some(out);
    }
    let bin = state.kepubify.as_ref()?;
    let _slot = KEPUB_GATE.acquire().await.ok()?;
    // Made meanwhile by the request that held the slot before this one.
    if tokio::fs::try_exists(&out).await.unwrap_or(false) {
        return Some(out);
    }
    tokio::fs::create_dir_all(&out_dir).await.ok()?;
    let src = state.data_dir.join("books").join(format!("{uuid}.epub"));
    let result = tokio::time::timeout(
        KEPUB_TIMEOUT,
        tokio::process::Command::new(bin).arg(&src).arg("-o").arg(&out_dir).kill_on_drop(true).output(),
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!("kepubify took longer than {}s for {uuid}; given up", KEPUB_TIMEOUT.as_secs());
            return None;
        }
    };
    match result {
        Ok(output) if output.status.success() => {
            // kepubify names its output <stem>_converted.kepub.epub; move it
            // to the stable cache name we serve from.
            let produced = out_dir.join(format!("{uuid}_converted.kepub.epub"));
            if tokio::fs::try_exists(&produced).await.unwrap_or(false) {
                tokio::fs::rename(&produced, &out).await.ok()?;
            }
            tokio::fs::try_exists(&out).await.unwrap_or(false).then_some(out)
        }
        Ok(output) => {
            tracing::warn!("kepubify failed for {uuid}: {}", String::from_utf8_lossy(&output.stderr));
            None
        }
        Err(e) => {
            tracing::warn!("kepubify could not run: {e}");
            None
        }
    }
}

pub async fn download(
    State(state): State<AppState>,
    Path((token, filename)): Path<(String, String)>,
) -> Result<Response, Response> {
    let result = download_file(&state, &token, &filename).await;
    let outcome: Result<Value, u16> = match &result {
        Ok((_, title, format)) => Ok(json!({ "title": title, "format": format })),
        Err(response) => Err(response.status().as_u16()),
    };
    let actor = actor_for_token(&state, &token).await;
    let who = actor.as_ref().map(|(id, name)| (*id, name.as_str()));
    match outcome {
        Ok(details) => crate::audit::log(&state, who, "kobo.downloaded", details).await,
        Err(status) => {
            tracing::warn!("kobo download of {filename} failed: HTTP {status}");
            crate::audit::log(&state, who, "kobo.download_failed", json!({ "file": filename, "status": status })).await;
        }
    }
    result.map(|(response, _, _)| response)
}

/// The file, the book's title and the format actually served ("kepub", or
/// "epub" also when the kepub conversion was unavailable).
async fn download_file(state: &AppState, token: &str, filename: &str) -> Result<(Response, String, &'static str), Response> {
    let user_id = user_from_token(state, token).await?;
    let want_kepub = filename.ends_with(".kepub.epub");
    let uuid = filename
        .strip_suffix(".kepub.epub")
        .or_else(|| filename.strip_suffix(".epub"))
        .unwrap_or(filename);
    let book = book_by_uuid(state, user_id, uuid).await?;

    if book.format == "cbz" {
        // A comic has no file a device can read but the one made here.
        let path = ensure_comic(state, &book).await.ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
        crate::kosync::remember_file_id(state, book.id, path.clone()).await;
        return Ok((epub_response(&path).await?, book.title, "kepub"));
    }
    let epub_path = state.data_dir.join("books").join(format!("{}.epub", book.uuid));
    let kepub = if want_kepub { ensure_kepub(state, &book.uuid).await } else { None };
    // KOReader on the same device knows the book by the file it was handed.
    if let Some(path) = &kepub {
        crate::kosync::remember_file_id(state, book.id, path.clone()).await;
    }
    let format = if kepub.is_some() { "kepub" } else { "epub" };
    Ok((epub_response(&kepub.unwrap_or(epub_path)).await?, book.title, format))
}

/// The file streamed from disk, with its length: a comic made into a book
/// for the device can be hundreds of MB.
async fn epub_response(path: &std::path::Path) -> Result<Response, Response> {
    let (len, body) = crate::books::file_body(path).await.map_err(|e| internal(e.into()))?;
    Ok(([(header::CONTENT_TYPE, "application/epub+zip".to_string()), (header::CONTENT_LENGTH, len.to_string())], body).into_response())
}

async fn serve_cover(state: &AppState, token: &str, image_id: &str) -> Result<Response, Response> {
    let user_id = user_from_token(state, token).await?;
    let book = book_by_uuid(state, user_id, image_id).await?;
    let mime: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM books WHERE id = $1")
        .bind(book.id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let mime = mime.ok_or_else(not_found)?;
    let data = tokio::fs::read(state.data_dir.join("covers").join(&book.uuid))
        .await
        .map_err(|_| not_found())?;
    Ok((crate::images::headers(&mime), data).into_response())
}

/// One info line per cover request, without the token. TraceLayer only logs
/// at debug, which hides covers that fail to reach the device.
fn log_cover(image_id: &str, size: &str, result: &Result<Response, Response>) {
    let r = match result {
        Ok(r) | Err(r) => r,
    };
    let mime = r
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let bytes = r
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    tracing::info!("kobo cover {image_id} {size}: {} {mime} {bytes} B", r.status().as_u16());
}

pub async fn image(
    State(state): State<AppState>,
    Path((token, image_id, w, h, _grey)): Path<(String, String, String, String, String)>,
) -> Result<Response, Response> {
    let result = serve_cover(&state, &token, &image_id).await;
    log_cover(&image_id, &format!("{w}x{h}"), &result);
    result
}

pub async fn image_quality(
    State(state): State<AppState>,
    Path((token, image_id, w, h, q, _grey)): Path<(String, String, String, String, String, String)>,
) -> Result<Response, Response> {
    let result = serve_cover(&state, &token, &image_id).await;
    log_cover(&image_id, &format!("{w}x{h} q{q}"), &result);
    result
}

/// One info line per device request: user, method, path, status and timing.
/// The path is relative to the nest, so the token is never logged. Cover
/// requests are logged separately by log_cover.
pub async fn access_log(
    State(state): State<AppState>,
    original: axum::extract::OriginalUri,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let agent = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .and_then(|ua| ua.rsplit_once('(').map(|(_, device)| device.trim_end_matches(')').to_string()))
        .unwrap_or_else(|| "-".into());
    let token = original.0.path().strip_prefix("/api/kobo/").and_then(|rest| rest.split('/').next()).unwrap_or("").to_string();
    let names_device = device_of(request.headers()).is_some();
    let actor = if names_device || !path.ends_with("/image.jpg") { actor_for_token(&state, &token).await } else { None };
    // A request that names the device ties it to the token's user: that is
    // how its requests without the token (reading services) are known.
    if let (true, Some((user_id, _))) = (names_device, &actor) {
        remember_device(&state, *user_id, request.headers()).await;
    }
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    if !path.ends_with("/image.jpg") {
        let user = actor.map(|(_, name)| name).unwrap_or_else(|| "?".into());
        tracing::info!(
            "kobo {user} {method} {path}: {} in {} ms ({agent})",
            response.status().as_u16(),
            started.elapsed().as_millis()
        );
    }
    response
}

#[cfg(test)]
mod probe_tests {
    use super::without_token;

    #[test]
    fn the_sync_token_never_reaches_the_log() {
        assert_eq!(without_token("/api/kobo/abc123/api/v3/content/x/annotations"), "/api/kobo/…/api/v3/content/x/annotations");
        assert_eq!(without_token("/api/kobo/abc123"), "/api/kobo/…");
        assert_eq!(without_token("/api/v3/content/x/annotations"), "/api/v3/content/x/annotations");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_time_compares_with_the_databases_own() {
        assert_eq!(device_timestamp("2026-10-12T10:51:24Z").as_deref(), Some("2026-10-12T10:51:24.000Z"));
        assert_eq!(device_timestamp("2026-10-12T10:51:24.217620743Z").as_deref(), Some("2026-10-12T10:51:24.217Z"));
        assert_eq!(device_timestamp("2026-10-12T12:51:24+02:00").as_deref(), Some("2026-10-12T10:51:24.000Z"));
        assert_eq!(device_timestamp("yesterday"), None);
    }

    #[test]
    fn timestamps_are_cut_to_milliseconds() {
        let mut v = json!({
            "LastModified": "2026-10-03T18:56:19.217620743Z",
            "Nested": [{ "Created": "2026-10-02T21:37:25.167704936Z" }],
            "Fine": "2026-10-03T18:56:19.217Z",
            "Seconds": "2026-10-03T18:56:19Z",
            "Title": "2026-10-03T18:56:19.217620743 är inte en tidsstämpel",
            "Number": 7
        });
        trim_timestamps(&mut v);
        assert_eq!(v["LastModified"], "2026-10-03T18:56:19.217Z");
        assert_eq!(v["Nested"][0]["Created"], "2026-10-02T21:37:25.167Z");
        assert_eq!(v["Fine"], "2026-10-03T18:56:19.217Z");
        assert_eq!(v["Seconds"], "2026-10-03T18:56:19Z");
        assert_eq!(v["Title"], "2026-10-03T18:56:19.217620743 är inte en tidsstämpel");
        assert_eq!(now().len(), 24);
    }
}
