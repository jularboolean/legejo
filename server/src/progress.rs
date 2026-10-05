//! Reading position for the web reader, per user+book.
//!
//! The web reader stores an EPUB CFI plus an overall percentage (0–1) in
//! `reading_progress`. Kobo devices keep their own state in
//! `kobo_reading_state`, with a location format that can't be mapped to a CFI.
//! KOReader (via kosync.rs) reports an xpointer and a percentage into
//! `kosync_progress`. Only the percentage is shared, and only when reading:
//! whichever of the three sources was updated last decides the percent shown.
//! Nothing here writes to the device tables, so device sync is unaffected.

use crate::auth::AuthUser;
use crate::db::{now_ts, Backend};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

const MAX_CFI_CHARS: usize = 2000;

/// Joins the three position tables onto `books b`. `$1` must be the user id.
/// kosync_progress has at most one row per user and book (see the migration).
pub(crate) const PROGRESS_JOINS: &str =
    "LEFT JOIN reading_progress rp ON rp.book_id = b.id AND rp.user_id = $1
     LEFT JOIN kobo_reading_state ks ON ks.book_id = b.id AND ks.user_id = $1
     LEFT JOIN kosync_progress kp ON kp.book_id = b.id AND kp.user_id = $1";

/// Kobo's overall progress as 0–1, or NULL when the device hasn't reported
/// one. Kobo sends ProgressPercent as 0–100; a finished book counts as fully
/// read. The JSON access is dialect-specific: SQLite's json_extract vs
/// Postgres' jsonb operators, with try_jsonb (defined in the Postgres
/// migrations) standing in for json_valid.
fn kobo_percent(backend: Backend) -> &'static str {
    match backend {
        Backend::Sqlite => {
            "(CASE WHEN ks.state IS NULL OR NOT json_valid(ks.state) THEN NULL \
              WHEN json_extract(ks.state, '$.StatusInfo.Status') = 'Finished' THEN 1.0 \
              ELSE MIN(1.0, MAX(0.0, CAST(json_extract(ks.state, '$.CurrentBookmark.ProgressPercent') AS REAL) / 100.0)) END)"
        }
        Backend::Postgres => {
            "(CASE WHEN ks.state IS NULL OR try_jsonb(ks.state) IS NULL THEN NULL \
              WHEN try_jsonb(ks.state) #>> '{StatusInfo,Status}' = 'Finished' THEN 1.0 \
              WHEN jsonb_typeof(try_jsonb(ks.state) #> '{CurrentBookmark,ProgressPercent}') = 'number' \
                THEN LEAST(1.0, GREATEST(0.0, (try_jsonb(ks.state) #>> '{CurrentBookmark,ProgressPercent}')::double precision / 100.0)) \
              ELSE NULL END)"
        }
    }
}

/// True when the KOReader position is newer than both the web's and a Kobo
/// position that carries a percent.
pub(crate) fn koreader_wins(backend: Backend) -> String {
    let kobo = kobo_percent(backend);
    format!(
        "(kp.updated_at IS NOT NULL AND (rp.updated_at IS NULL OR kp.updated_at > rp.updated_at)
          AND ({kobo} IS NULL OR kp.updated_at > ks.updated_at))"
    )
}

/// True when the Kobo position is the most recent one that carries a percent.
pub(crate) fn kobo_wins(backend: Backend) -> String {
    let percent = kobo_percent(backend);
    format!(
        "({percent} IS NOT NULL AND (rp.updated_at IS NULL OR ks.updated_at > rp.updated_at) AND NOT {})",
        koreader_wins(backend)
    )
}

/// Latest known progress (0–1) across web, Kobo and KOReader; needs `PROGRESS_JOINS`.
pub(crate) fn progress_percent(backend: Backend) -> String {
    let kobo = kobo_percent(backend);
    format!(
        "CASE WHEN {ko} THEN kp.percentage WHEN {kw} THEN {kobo} ELSE rp.percent END",
        ko = koreader_wins(backend),
        kw = kobo_wins(backend),
    )
}

/// When the winning source was last updated; needs `PROGRESS_JOINS`.
pub(crate) fn last_read_at(backend: Backend) -> String {
    format!(
        "CASE WHEN {ko} THEN kp.updated_at WHEN {kw} THEN ks.updated_at ELSE rp.updated_at END",
        ko = koreader_wins(backend),
        kw = kobo_wins(backend),
    )
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))).into_response()
}

fn unprocessable(msg: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": msg }))).into_response()
}

/// Progress for one book, for responses built from queries without the joins.
pub(crate) async fn percent_for(state: &AppState, user_id: i64, book_id: i64) -> Result<Option<f64>, sqlx::Error> {
    let percent: Option<Option<f64>> = sqlx::query_scalar(&format!(
        "SELECT {percent} FROM books b {PROGRESS_JOINS} WHERE b.id = $2",
        percent = progress_percent(state.backend),
    ))
    .bind(user_id)
    .bind(book_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(percent.flatten())
}

#[derive(Serialize)]
pub struct Progress {
    /// Position in the web reader. Null when there is none, or when a Kobo or
    /// KOReader device has reported a newer position (which has no CFI).
    cfi: Option<String>,
    /// 0–1, from whichever of web, Kobo and KOReader was updated last.
    percent: Option<f64>,
    updated_at: Option<String>,
}

pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Progress>, Response> {
    let row: Option<(Option<String>, Option<f64>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT CASE WHEN {kobo_wins} OR {ko_wins} THEN NULL ELSE rp.cfi END,
                {percent},
                {last}
         FROM books b {PROGRESS_JOINS}
         WHERE b.owner_id = $1 AND b.id = $2",
        kobo_wins = kobo_wins(state.backend),
        ko_wins = koreader_wins(state.backend),
        percent = progress_percent(state.backend),
        last = last_read_at(state.backend),
    ))
    .bind(user.0.id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let (cfi, percent, updated_at) = row.ok_or_else(not_found)?;
    Ok(Json(Progress { cfi, percent, updated_at }))
}

#[derive(Deserialize)]
pub struct PutProgress {
    cfi: String,
    percent: f64,
}

pub async fn put(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<PutProgress>,
) -> Result<StatusCode, Response> {
    if !(0.0..=1.0).contains(&req.percent) {
        return Err(unprocessable("percent must be between 0 and 1"));
    }
    if req.cfi.chars().count() > MAX_CFI_CHARS {
        return Err(unprocessable("cfi too long (max 2000 characters)"));
    }

    // Insert only for a book the user owns; no row written means no such book.
    let result = sqlx::query(
        "INSERT INTO reading_progress (user_id, book_id, cfi, percent, updated_at)
         SELECT $1, id, $3, $4, $5
         FROM books WHERE id = $2 AND owner_id = $1
         ON CONFLICT (user_id, book_id)
         DO UPDATE SET cfi = excluded.cfi, percent = excluded.percent, updated_at = excluded.updated_at",
    )
    .bind(user.0.id)
    .bind(id)
    .bind(&req.cfi)
    .bind(req.percent)
    .bind(now_ts())
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::db::Backend;
    use crate::{router, AppState};
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use axum::Router;
    use serde_json::{json, Value};
    use sqlx::AnyPool;
    use tower::ServiceExt;

    /// App on a fresh in-memory database with all migrations applied.
    async fn test_app() -> (Router, AnyPool) {
        sqlx::any::install_default_drivers();
        // One connection that never retires: every query must see the same
        // in-memory database for the whole test.
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await.unwrap();
        crate::db::SQLITE_MIGRATOR.run(&db).await.unwrap();
        let state = AppState {
            db: db.clone(),
            backend: Backend::Sqlite,
            data_dir: std::env::temp_dir(),
            kepubify: None,
            http: reqwest::Client::new(),
            mail: None,
            thumb_gate: std::sync::Arc::new(tokio::sync::Semaphore::new(2)),
            fed: crate::fed::Fed::disabled(),
            export_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            oidc: None,
            settings: std::sync::Arc::new(crate::settings::Settings::default()),
        };
        (router(state), db)
    }

    /// Creates a user with a live session; returns (user id, Cookie header value).
    async fn add_user(db: &AnyPool, name: &str) -> (i64, String) {
        let id: i64 = sqlx::query_scalar("INSERT INTO users (username, password_hash) VALUES (?, 'x') RETURNING id")
            .bind(name)
            .fetch_one(db)
            .await
            .unwrap();
        let token = format!("session-{name}");
        sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, '2999-01-01T00:00:00.000Z')")
            .bind(&token)
            .bind(id)
            .execute(db)
            .await
            .unwrap();
        (id, format!("{}={token}", crate::auth::SESSION_COOKIE))
    }

    async fn add_book(db: &AnyPool, owner_id: i64, title: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO books (uuid, owner_id, title, file_size) VALUES (?, ?, ?, 1) RETURNING id")
            .bind(format!("uuid-{owner_id}-{title}"))
            .bind(owner_id)
            .bind(title)
            .fetch_one(db)
            .await
            .unwrap()
    }

    /// Stores a Kobo reading state the way kobo::reading_state_put does.
    async fn set_kobo_state(db: &AnyPool, user_id: i64, book_id: i64, state: Value, updated_at: &str) {
        sqlx::query(
            "INSERT INTO kobo_reading_state (user_id, book_id, state, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (user_id, book_id)
             DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at",
        )
        .bind(user_id)
        .bind(book_id)
        .bind(state.to_string())
        .bind(updated_at)
        .execute(db)
        .await
        .unwrap();
    }

    async fn send(app: &Router, method: Method, uri: &str, cookie: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            req = req.header(header::COOKIE, cookie);
        }
        let req = match body {
            Some(body) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn get(app: &Router, uri: &str, cookie: &str) -> (StatusCode, Value) {
        send(app, Method::GET, uri, Some(cookie), None).await
    }

    async fn put(app: &Router, uri: &str, cookie: &str, body: Value) -> StatusCode {
        send(app, Method::PUT, uri, Some(cookie), Some(body)).await.0
    }

    #[tokio::test]
    async fn requires_session() {
        let (app, db) = test_app().await;
        let (user, _) = add_user(&db, "anna").await;
        let book = add_book(&db, user, "Bok").await;
        let uri = format!("/api/books/{book}/progress");

        let (status, _) = send(&app, Method::GET, &uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = send(&app, Method::PUT, &uri, None, Some(json!({ "cfi": "x", "percent": 0.1 }))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_then_roundtrip() {
        let (app, db) = test_app().await;
        let (user, cookie) = add_user(&db, "anna").await;
        let book = add_book(&db, user, "Bok").await;
        let uri = format!("/api/books/{book}/progress");

        let (status, body) = get(&app, &uri, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "cfi": null, "percent": null, "updated_at": null }));

        let cfi = "epubcfi(/6/4!/4/2/1:0)";
        assert_eq!(put(&app, &uri, &cookie, json!({ "cfi": cfi, "percent": 0.42 })).await, StatusCode::NO_CONTENT);
        let (status, body) = get(&app, &uri, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["cfi"], cfi);
        assert_eq!(body["percent"], 0.42);
        assert!(body["updated_at"].as_str().unwrap().ends_with('Z'));

        // A second PUT replaces the position rather than adding a row.
        assert_eq!(put(&app, &uri, &cookie, json!({ "cfi": "epubcfi(/6/8)", "percent": 1 })).await, StatusCode::NO_CONTENT);
        let (_, body) = get(&app, &uri, &cookie).await;
        assert_eq!(body["cfi"], "epubcfi(/6/8)");
        assert_eq!(body["percent"], 1.0);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reading_progress").fetch_one(&db).await.unwrap();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn unknown_or_foreign_book_is_404() {
        let (app, db) = test_app().await;
        let (anna, anna_cookie) = add_user(&db, "anna").await;
        let (_, bo_cookie) = add_user(&db, "bo").await;
        let book = add_book(&db, anna, "Bok").await;
        let body = json!({ "cfi": "x", "percent": 0.5 });

        assert_eq!(get(&app, "/api/books/9999/progress", &anna_cookie).await.0, StatusCode::NOT_FOUND);
        assert_eq!(put(&app, "/api/books/9999/progress", &anna_cookie, body.clone()).await, StatusCode::NOT_FOUND);

        let uri = format!("/api/books/{book}/progress");
        assert_eq!(get(&app, &uri, &bo_cookie).await.0, StatusCode::NOT_FOUND);
        assert_eq!(put(&app, &uri, &bo_cookie, body).await, StatusCode::NOT_FOUND);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reading_progress").fetch_one(&db).await.unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn validates_input() {
        let (app, db) = test_app().await;
        let (user, cookie) = add_user(&db, "anna").await;
        let book = add_book(&db, user, "Bok").await;
        let uri = format!("/api/books/{book}/progress");

        for body in [
            json!({ "cfi": "x", "percent": 1.01 }),
            json!({ "cfi": "x", "percent": -0.01 }),
            json!({ "cfi": "x".repeat(2001), "percent": 0.5 }),
            json!({ "cfi": "x" }),
            json!({ "percent": 0.5 }),
            json!({ "cfi": "x", "percent": "0.5" }),
        ] {
            assert_eq!(put(&app, &uri, &cookie, body.clone()).await, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        }
        // The limits themselves are accepted.
        assert_eq!(put(&app, &uri, &cookie, json!({ "cfi": "x".repeat(2000), "percent": 0 })).await, StatusCode::NO_CONTENT);
        assert_eq!(put(&app, &uri, &cookie, json!({ "cfi": "å".repeat(2000), "percent": 1 })).await, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn book_json_carries_progress_percent() {
        let (app, db) = test_app().await;
        let (user, cookie) = add_user(&db, "anna").await;
        let read = add_book(&db, user, "Läst").await;
        let unread = add_book(&db, user, "Oläst").await;
        let shelf: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name) VALUES (?, 'Hylla') RETURNING id")
            .bind(user)
            .fetch_one(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO shelf_books (shelf_id, book_id) VALUES (?, ?), (?, ?)")
            .bind(shelf)
            .bind(read)
            .bind(shelf)
            .bind(unread)
            .execute(&db)
            .await
            .unwrap();
        put(&app, &format!("/api/books/{read}/progress"), &cookie, json!({ "cfi": "x", "percent": 0.25 })).await;

        let (_, body) = get(&app, &format!("/api/books/{read}"), &cookie).await;
        assert_eq!(body["progress_percent"], 0.25);
        assert_eq!(body["title"], "Läst");
        // Omitted, not null, for a book without a position.
        let (_, body) = get(&app, &format!("/api/books/{unread}"), &cookie).await;
        assert!(body.get("progress_percent").is_none());

        let by_id = |list: &Value, id: i64| list.as_array().unwrap().iter().find(|b| b["id"] == id).cloned().unwrap();
        let (_, list) = get(&app, "/api/books", &cookie).await;
        assert_eq!(list.as_array().unwrap().len(), 2);
        // The listing carries shelf membership for the library's shelf filter.
        assert_eq!(by_id(&list, read)["shelf_ids"], json!([shelf]));
        assert_eq!(by_id(&list, read)["progress_percent"], 0.25);
        assert!(by_id(&list, unread).get("progress_percent").is_none());

        let (_, found) = get(&app, "/api/books?q=L%C3%A4st", &cookie).await;
        assert_eq!(found.as_array().unwrap().len(), 1);
        assert_eq!(found[0]["progress_percent"], 0.25);

        let (_, detail) = get(&app, &format!("/api/shelves/{shelf}"), &cookie).await;
        assert_eq!(by_id(&detail["books"], read)["progress_percent"], 0.25);

        // The edit response keeps the field too.
        let (status, body) =
            send(&app, Method::PUT, &format!("/api/books/{read}"), Some(&cookie), Some(json!({ "title": "Ny titel" }))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["progress_percent"], 0.25);
    }

    #[tokio::test]
    async fn latest_of_web_and_kobo_decides_percent() {
        let (app, db) = test_app().await;
        let (user, cookie) = add_user(&db, "anna").await;
        let book = add_book(&db, user, "Bok").await;
        let uri = format!("/api/books/{book}/progress");
        let kobo = json!({
            "StatusInfo": { "Status": "Reading" },
            "CurrentBookmark": { "ProgressPercent": 42, "Location": { "Value": "kobo.1.1", "Type": "KoboSpan" } },
        });

        // Kobo only: percent is known, but there is no CFI to open at.
        set_kobo_state(&db, user, book, kobo.clone(), "2001-01-01T00:00:00.000Z").await;
        let (_, body) = get(&app, &uri, &cookie).await;
        assert_eq!(body, json!({ "cfi": null, "percent": 0.42, "updated_at": "2001-01-01T00:00:00.000Z" }));
        let (_, body) = get(&app, &format!("/api/books/{book}"), &cookie).await;
        assert_eq!(body["progress_percent"], 0.42);

        // Reading on the web afterwards takes over.
        put(&app, &uri, &cookie, json!({ "cfi": "epubcfi(/6/4)", "percent": 0.5 })).await;
        let (_, body) = get(&app, &uri, &cookie).await;
        assert_eq!(body["cfi"], "epubcfi(/6/4)");
        assert_eq!(body["percent"], 0.5);

        // ...until the device reports a newer position.
        set_kobo_state(&db, user, book, kobo, "2999-01-01T00:00:00.000Z").await;
        let (_, body) = get(&app, &uri, &cookie).await;
        assert_eq!(body, json!({ "cfi": null, "percent": 0.42, "updated_at": "2999-01-01T00:00:00.000Z" }));
        let (_, list) = get(&app, "/api/books", &cookie).await;
        assert_eq!(list[0]["progress_percent"], 0.42);

        // A finished book is fully read, whatever the bookmark says.
        let finished = json!({ "StatusInfo": { "Status": "Finished" }, "CurrentBookmark": {} });
        set_kobo_state(&db, user, book, finished, "2999-01-02T00:00:00.000Z").await;
        assert_eq!(get(&app, &uri, &cookie).await.1["percent"], 1.0);

        // A newer Kobo state without any progress doesn't hide the web position.
        let untouched = json!({ "StatusInfo": { "Status": "ReadyToRead" }, "CurrentBookmark": {} });
        set_kobo_state(&db, user, book, untouched, "2999-01-03T00:00:00.000Z").await;
        let (_, body) = get(&app, &uri, &cookie).await;
        assert_eq!(body["cfi"], "epubcfi(/6/4)");
        assert_eq!(body["percent"], 0.5);

        // Garbage in the Kobo table is ignored rather than failing the request.
        sqlx::query("UPDATE kobo_reading_state SET state = 'not json'").execute(&db).await.unwrap();
        let (status, body) = get(&app, &uri, &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["percent"], 0.5);
    }

    #[tokio::test]
    async fn web_progress_leaves_kobo_state_alone() {
        let (app, db) = test_app().await;
        let (user, cookie) = add_user(&db, "anna").await;
        let book = add_book(&db, user, "Bok").await;
        let kobo = json!({ "CurrentBookmark": { "ProgressPercent": 10 } });
        set_kobo_state(&db, user, book, kobo.clone(), "2001-01-01T00:00:00.000Z").await;

        put(&app, &format!("/api/books/{book}/progress"), &cookie, json!({ "cfi": "x", "percent": 0.9 })).await;

        let (state, updated_at): (String, String) =
            sqlx::query_as("SELECT state, updated_at FROM kobo_reading_state").fetch_one(&db).await.unwrap();
        assert_eq!(state, kobo.to_string());
        assert_eq!(updated_at, "2001-01-01T00:00:00.000Z");
    }

    #[tokio::test]
    async fn progress_is_per_user_and_removed_with_book_or_user() {
        let (app, db) = test_app().await;
        let (anna, anna_cookie) = add_user(&db, "anna").await;
        let (bo, bo_cookie) = add_user(&db, "bo").await;
        let anna_book = add_book(&db, anna, "Annas").await;
        let bo_book = add_book(&db, bo, "Bos").await;
        put(&app, &format!("/api/books/{anna_book}/progress"), &anna_cookie, json!({ "cfi": "a", "percent": 0.1 })).await;
        put(&app, &format!("/api/books/{bo_book}/progress"), &bo_cookie, json!({ "cfi": "b", "percent": 0.2 })).await;
        let count = || async {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reading_progress").fetch_one(&db).await.unwrap()
        };
        assert_eq!(count().await, 2);

        let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{anna_book}"), Some(&anna_cookie), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(count().await, 1);

        sqlx::query("DELETE FROM users WHERE id = ?").bind(bo).execute(&db).await.unwrap();
        assert_eq!(count().await, 0);
    }
}
