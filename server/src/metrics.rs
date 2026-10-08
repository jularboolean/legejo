//! Prometheus metrics at /metrics. Off unless LEGEJO_METRICS=true (open)
//! or LEGEJO_METRICS_TOKEN (bearer token) is set.
//!
//! Every value is read from the database at scrape time, so all server
//! processes report the same numbers.

use crate::settings::Metrics;
use crate::AppState;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write;

fn authorized(state: &AppState, headers: &HeaderMap) -> Option<bool> {
    match &state.settings.metrics {
        Metrics::Off => None,
        Metrics::Open => Some(true),
        Metrics::Token(token) => {
            let given = headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .unwrap_or("");
            // Compare digests: no early exit on the first differing byte.
            Some(Sha256::digest(given.as_bytes()) == Sha256::digest(token.as_bytes()))
        }
    }
}

async fn scalar(state: &AppState, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(&state.db).await.unwrap_or_else(|e| {
        tracing::warn!("metrics: {sql}: {e}");
        0
    })
}

fn label(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
}

fn gauge(out: &mut String, name: &str, help: &str, value: i64) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} gauge\n{name} {value}");
}

pub async fn metrics(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match authorized(&state, &headers) {
        None => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Some(false) => return (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Bearer")], "unauthorized").into_response(),
        Some(true) => {}
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# HELP legejo_info Build information.\n# TYPE legejo_info gauge\nlegejo_info{{version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION")
    );
    gauge(&mut out, "legejo_users", "Accounts.", scalar(&state, "SELECT COUNT(*) FROM users").await);
    gauge(&mut out, "legejo_books", "Books in all libraries.", scalar(&state, "SELECT COUNT(*) FROM books").await);
    gauge(
        &mut out,
        "legejo_books_bytes",
        "Total size of the EPUB files.",
        scalar(&state, "SELECT CAST(COALESCE(SUM(file_size), 0) AS BIGINT) FROM books").await,
    );
    gauge(&mut out, "legejo_shelves", "Shelves.", scalar(&state, "SELECT COUNT(*) FROM shelves").await);
    gauge(&mut out, "legejo_shelves_public", "Public shelves.", scalar(&state, "SELECT COUNT(*) FROM shelves WHERE is_public <> 0").await);
    gauge(
        &mut out,
        "legejo_federation_deliveries_pending",
        "ActivityPub deliveries waiting to be sent or retried.",
        scalar(&state, "SELECT COUNT(*) FROM ap_deliveries").await,
    );
    gauge(
        &mut out,
        "legejo_login_failures",
        "Failed logins (web and OPDS) within the throttling window.",
        crate::ratelimit::recent_failures(&state).await,
    );

    let exports: Vec<(String, i64)> = sqlx::query_as("SELECT status, COUNT(*) FROM exports GROUP BY status")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let _ = writeln!(out, "# HELP legejo_exports Library exports by status.\n# TYPE legejo_exports gauge");
    for status in ["queued", "running", "done", "failed", "expired"] {
        let n = exports.iter().find(|(s, _)| s == status).map_or(0, |(_, n)| *n);
        let _ = writeln!(out, "legejo_exports{{status=\"{status}\"}} {n}");
    }

    // The system log as activity: events of the last 24 hours per action.
    let since = crate::db::ts_in_hours(-24);
    let events: Vec<(String, i64)> = sqlx::query_as("SELECT action, COUNT(*) FROM activity_log WHERE at > $1 GROUP BY action")
        .bind(since)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let events: BTreeMap<String, i64> = events.into_iter().collect();
    let _ = writeln!(out, "# HELP legejo_events_24h System-log events in the last 24 hours, per action.\n# TYPE legejo_events_24h gauge");
    for (action, n) in &events {
        let _ = writeln!(out, "legejo_events_24h{{action=\"{}\"}} {n}", label(action));
    }

    if let Some(import) = &state.settings.import {
        gauge(&mut out, "legejo_import_pending", "EPUB files waiting in the import folder.", crate::importdir::pending(&import.dir) as i64);
    }
    if let Some(import) = &state.settings.audio_import {
        gauge(&mut out, "legejo_audiobook_import_pending", "Audiobooks waiting in the audiobook import folder.", crate::audioimport::pending(&import.dir) as i64);
    }

    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], out).into_response()
}
