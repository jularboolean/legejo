//! Everything said with a Kobo, for finding out why a sync went wrong: off
//! unless LEGEJO_KOBO_TRACE=true. One line for each request and one for each
//! answer, with headers and bodies, numbered so that they can be paired.
//!
//! What can open an account stays out of the log: the sync token, wherever it
//! is written, and the values the device or Kobo keeps as keys. So do the
//! reader's address and what the device reports of its use: searches, words
//! looked up, the account's profile. What sync carries stays in, since that is
//! what is looked for here: titles, places and the text of highlights.

use crate::kobo;
use crate::AppState;
use axum::body::{Body, HttpBody};
use axum::extract::{OriginalUri, Request, State};
use axum::http::{header, HeaderMap};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Largest body read to be logged. A larger one passes unread.
const READ_AT_MOST: u64 = 16 * 1024 * 1024;
/// How much of a body reaches the log.
const SHOWN: usize = 64 * 1024;

/// Headers that say where the reader is.
const ADDRESS_HEADERS: [&str; 5] = ["cf-connecting-ip", "cf-ipcountry", "x-forwarded-for", "x-real-ip", "forwarded"];

/// Paths whose bodies are about the person and not about sync.
const PRIVATE_PATHS: [&str; 3] = ["/v1/analytics/", "/v1/user/", "/checkforchanges"];

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A JSON field whose value is a key to something.
fn is_secret(field: &str) -> bool {
    let field = field.to_lowercase();
    field.ends_with("token") || field.ends_with("key") || field.contains("secret") || field.contains("password")
}

/// The JSON with its secrets out. An address keeps no query: Kobo's links to
/// bought books are signed there.
fn without_secrets(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (field, value) in fields.iter_mut() {
                if is_secret(field) && !value.is_null() {
                    *value = Value::String("<hidden>".into());
                } else {
                    without_secrets(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(without_secrets),
        Value::String(text) if text.starts_with("http") => {
            if let Some((address, _)) = text.split_once('?') {
                *text = format!("{address}?…");
            }
        }
        _ => {}
    }
}

/// The text as it may be logged: no token in it, and not too long.
fn cleaned(text: &str, token: &str) -> String {
    let text = if token.is_empty() { text.to_string() } else { text.replace(token, "…") };
    if text.len() <= SHOWN {
        return text;
    }
    let end = (0..=SHOWN).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
    format!("{}… ({} bytes in all)", &text[..end], text.len())
}

/// A body as it may be logged, on one line.
fn body_text(bytes: &[u8], token: &str) -> String {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(mut value) => {
            without_secrets(&mut value);
            cleaned(&value.to_string(), token)
        }
        Err(_) => cleaned(&String::from_utf8_lossy(bytes).replace('\n', "\\n"), token),
    }
}

/// Headers as they may be logged.
fn headers_text(headers: &HeaderMap, token: &str) -> String {
    let shown: Vec<String> = headers
        .iter()
        .map(|(name, value)| {
            let name = name.as_str();
            let text = value.to_str().unwrap_or("<binary>");
            if ADDRESS_HEADERS.contains(&name) {
                format!("{name}=<hidden>")
            } else if kobo::SECRET_HEADERS.contains(&name) || name == "set-cookie" {
                format!("{name}=<{} bytes>", value.len())
            } else if name == "x-kobo-synctoken" {
                format!("{name}={}", kobo::sync_token_shown(text))
            } else if name == "x-kobo-deviceid" {
                format!("{name}={}", text.chars().take(8).collect::<String>())
            } else {
                format!("{name}={}", cleaned(text, token))
            }
        })
        .collect();
    shown.join(" ")
}

/// Text is logged; a book or a cover is only named.
fn is_text(headers: &HeaderMap) -> bool {
    let kind = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let plain = !headers.contains_key(header::CONTENT_ENCODING);
    plain && (kind.is_empty() || kind.contains("json") || kind.contains("xml") || kind.contains("text") || kind.contains("form"))
}

/// The body for the log, and the body to pass on. Nothing that is long, or
/// comes as a stream, is held back.
async fn read(headers: &HeaderMap, body: Body, token: &str, private: bool) -> (String, Body) {
    if private {
        return ("<not logged>".into(), body);
    }
    let size = body.size_hint().upper();
    if !is_text(headers) || !size.is_some_and(|size| size <= READ_AT_MOST) {
        let kind = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("unknown type");
        return (format!("<{kind}, not read>"), body);
    }
    match axum::body::to_bytes(body, READ_AT_MOST as usize).await {
        Ok(bytes) => (body_text(&bytes, token), Body::from(bytes)),
        Err(e) => (format!("<could not be read: {e}>"), Body::empty()),
    }
}

/// A request and its answer in the log, when the trace is on.
pub async fn trace(State(state): State<AppState>, original: OriginalUri, request: Request, next: Next) -> Response {
    if !state.settings.kobo_trace {
        return next.run(request).await;
    }
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let token = original.0.path().strip_prefix("/api/kobo/").and_then(|rest| rest.split('/').next()).unwrap_or("").to_string();
    let path = cleaned(&original.0.to_string(), &token);
    let private = PRIVATE_PATHS.iter().any(|part| original.0.path().contains(part));
    let user = kobo::user_label(&state, &token, request.headers()).await;
    let (parts, body) = request.into_parts();
    let (shown, body) = read(&parts.headers, body, &token, private).await;
    tracing::info!(
        target: "legejo::kobo_trace",
        "kobo #{n} {user} {} {path} headers=[{}] body={shown}",
        parts.method,
        headers_text(&parts.headers, &token),
    );

    let started = std::time::Instant::now();
    let response = next.run(Request::from_parts(parts, body)).await;

    let (parts, body) = response.into_parts();
    let (shown, body) = read(&parts.headers, body, &token, private).await;
    tracing::info!(
        target: "legejo::kobo_trace",
        "kobo #{n} answer {} in {} ms headers=[{}] body={shown}",
        parts.status.as_u16(),
        started.elapsed().as_millis(),
        headers_text(&parts.headers, &token),
    );
    Response::from_parts(parts, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_and_signed_links_stay_out_of_the_log() {
        let body = json!({
            "UserKey": "abc", "AccessToken": "def", "TokenType": "Bearer",
            "Resources": { "library_sync": "https://x.se/api/kobo/tok123/v1/library/sync" },
            "Download": [{ "Url": "https://kobo.example/book.epub?sig=s3cret" }],
            "Location": { "Value": "kobo.1.1", "Text": "of his esophagus" },
        });
        let text = body_text(body.to_string().as_bytes(), "tok123");
        for secret in ["abc", "def", "tok123", "s3cret"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.contains("of his esophagus"));
        assert!(text.contains("https://kobo.example/book.epub?…"));
        assert!(text.contains(r#""TokenType":"Bearer""#));
    }

    #[test]
    fn secret_headers_show_only_their_length() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer abc".parse().unwrap());
        headers.insert("x-kobo-userkey", "abc".parse().unwrap());
        headers.insert("x-kobo-deviceid", "d9f26b8441cbf151".parse().unwrap());
        headers.insert("x-kobo-synctoken", "kobos-own".parse().unwrap());
        headers.insert("referer", "https://x.se/api/kobo/tok123/a".parse().unwrap());
        let text = headers_text(&headers, "tok123");
        for secret in ["abc", "kobos-own", "tok123", "8441cbf151"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.contains("x-kobo-deviceid=d9f26b84"));
    }

    #[test]
    fn the_readers_address_stays_out_of_the_log() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "192.0.2.7".parse().unwrap());
        headers.insert("x-forwarded-for", "192.0.2.7".parse().unwrap());
        assert!(!headers_text(&headers, "").contains("192.0.2.7"));
    }

    #[test]
    fn a_long_body_is_cut_and_says_so() {
        let text = body_text("å".repeat(SHOWN).as_bytes(), "");
        assert!(text.ends_with(&format!("({} bytes in all)", SHOWN * 2)));
        assert!(text.len() < SHOWN + 40);
    }

    #[test]
    fn a_book_is_not_text() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/epub+zip".parse().unwrap());
        assert!(!is_text(&headers));
        headers.insert(header::CONTENT_TYPE, "application/json; charset=utf-8".parse().unwrap());
        assert!(is_text(&headers));
        headers.insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
        assert!(!is_text(&headers));
    }

    /// What the log gets from one device request, with the trace on or off.
    async fn logged(on: bool) -> (String, axum::http::StatusCode, Value) {
        use crate::testutil::{add_user, send, test_app};
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Log(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Log {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let (_, db, mut state) = test_app().await;
        state.settings = Arc::new(crate::settings::Settings { kobo_trace: on, ..Default::default() });
        let app = crate::router(state);
        let (id, _) = add_user(&db, "alice").await;
        sqlx::query("UPDATE users SET kobo_token = 'tok123' WHERE id = $1").bind(id).execute(&db).await.unwrap();

        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt().with_ansi(false).with_writer(move || writer.clone()).finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        let body = json!({ "UserKey": "user-key-1", "DeviceId": "d9f26b84" });
        let (status, answer) = send(&app, axum::http::Method::POST, "/api/kobo/tok123/v1/auth/device", None, Some(body)).await;
        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        (text, status, answer)
    }

    #[tokio::test]
    async fn a_request_and_its_answer_reach_the_log_without_secrets() {
        let (log, status, answer) = logged(true).await;
        assert!(status.is_success());
        assert!(answer["AccessToken"].as_str().is_some_and(|t| !t.is_empty() && t != "<hidden>"), "the device gets its answer whole");
        let access = answer["AccessToken"].as_str().unwrap();
        assert!(log.contains("alice POST /api/kobo/…/v1/auth/device"), "{log}");
        assert!(log.contains(r#""DeviceId":"d9f26b84""#), "{log}");
        assert!(log.contains("answer 200"), "{log}");
        for secret in ["tok123", "user-key-1", access] {
            assert!(!log.contains(secret), "{secret} in {log}");
        }
    }

    #[tokio::test]
    async fn off_it_logs_no_bodies() {
        let (log, status, _) = logged(false).await;
        assert!(status.is_success());
        assert!(!log.contains("legejo::kobo_trace"), "{log}");
        assert!(!log.contains("UserKey"), "{log}");
    }
}
