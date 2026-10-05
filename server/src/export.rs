//! Library export: the metadata as CSV or JSON on request, and the whole
//! library (EPUB files, covers and metadata) as a background job producing
//! one or more zip parts.
//!
//! The `exports` row is both the job queue and the progress the client
//! polls. Workers claim a queued export with an atomic UPDATE, so two server
//! processes running side by side never build the same one, and a heartbeat
//! lets another process take over an export whose worker died. Parts are
//! written as temp file + rename under <data>/exports/<user>/<export>/,
//! capped at PART_LIMIT each, served with Range support (resumable
//! downloads) and removed after EXPORT_DAYS.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::progress;
use crate::AppState;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

const PART_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const EXPORT_DAYS: i64 = 7;
/// A running export whose heartbeat is older than this is taken over.
const STALE_SECS: i64 = 180;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
}

fn ts_offset(secs: i64) -> String {
    let t = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
    t.format(time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"))
        .unwrap_or_default()
}

// ------------------------------------------------------------- metadata

#[derive(Serialize, Clone)]
pub struct ExportBook {
    id: i64,
    title: String,
    author: Option<String>,
    series: Option<String>,
    series_index: Option<f64>,
    isbn: Option<String>,
    identifier: Option<String>,
    language: Option<String>,
    publisher: Option<String>,
    published: Option<String>,
    first_published: Option<i64>,
    category: Option<String>,
    description: Option<String>,
    tags: Vec<String>,
    shelves: Vec<String>,
    rating: Option<i64>,
    progress_percent: Option<f64>,
    want_to_read: bool,
    license: Option<String>,
    license_source_url: Option<String>,
    author_death_year: Option<i64>,
    added: String,
    /// The file's path inside the export zip.
    file: String,
    #[serde(skip)]
    uuid: String,
    #[serde(skip)]
    cover_mime: Option<String>,
    #[serde(skip)]
    file_size: i64,
}

#[derive(Serialize)]
pub struct ExportShelf {
    name: String,
    description: Option<String>,
    visibility: String,
    books: usize,
}

/// "Selma Lagerlöf - Nils Holgersson" as a safe file name.
fn file_base(author: Option<&str>, title: &str) -> String {
    let raw = match author {
        Some(a) if !a.trim().is_empty() => format!("{} - {title}", a.trim()),
        _ => title.to_string(),
    };
    let s: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() || " -.,'()&".contains(c) { c } else { '_' })
        .take(150)
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "book".into() } else { s }
}

pub async fn collect(state: &AppState, user_id: i64) -> anyhow::Result<(Vec<ExportBook>, Vec<ExportShelf>)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        uuid: String,
        title: String,
        author: Option<String>,
        series: Option<String>,
        series_index: Option<f64>,
        isbn: Option<String>,
        identifier: Option<String>,
        language: Option<String>,
        publisher: Option<String>,
        published: Option<String>,
        first_published: Option<i64>,
        category: Option<String>,
        description: Option<String>,
        rating: Option<i64>,
        progress_percent: Option<f64>,
        want_to_read: i64,
        license: Option<String>,
        license_source_url: Option<String>,
        author_death_year: Option<i64>,
        created_at: String,
        cover_mime: Option<String>,
        file_size: i64,
    }
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT b.id, b.uuid, b.title, b.author, b.series, b.series_index, b.isbn, b.identifier, b.language,
                b.publisher, b.published, b.first_published, b.category, b.description, b.rating, {percent} AS progress_percent,
                b.want_to_read, b.license, b.license_source_url, b.author_death_year, b.created_at, b.cover_mime,
                b.file_size
         FROM books b {joins}
         WHERE b.owner_id = $1 ORDER BY b.id",
        percent = progress::progress_percent(state.backend),
        joins = progress::PROGRESS_JOINS,
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let tags: Vec<(i64, String)> = sqlx::query_as(
        "SELECT t.book_id, t.tag FROM book_tags t JOIN books b ON b.id = t.book_id WHERE b.owner_id = $1 ORDER BY LOWER(t.tag)",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let on_shelves: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sb.book_id, s.name FROM shelf_books sb JOIN shelves s ON s.id = sb.shelf_id WHERE s.owner_id = $1 ORDER BY LOWER(s.name)",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let mut used = std::collections::HashSet::new();
    let books = rows
        .into_iter()
        .map(|r| {
            let base = file_base(r.author.as_deref(), &r.title);
            let mut file = format!("books/{base}.epub");
            let mut n = 2;
            while !used.insert(file.to_lowercase()) {
                file = format!("books/{base} ({n}).epub");
                n += 1;
            }
            ExportBook {
                tags: tags.iter().filter(|(id, _)| *id == r.id).map(|(_, t)| t.clone()).collect(),
                shelves: on_shelves.iter().filter(|(id, _)| *id == r.id).map(|(_, s)| s.clone()).collect(),
                id: r.id,
                title: r.title,
                author: r.author,
                series: r.series,
                series_index: r.series_index,
                isbn: r.isbn,
                identifier: r.identifier,
                language: r.language,
                publisher: r.publisher,
                published: r.published,
                first_published: r.first_published,
                category: r.category,
                description: r.description,
                rating: r.rating,
                progress_percent: r.progress_percent,
                want_to_read: r.want_to_read != 0,
                license: r.license,
                license_source_url: r.license_source_url,
                author_death_year: r.author_death_year,
                added: r.created_at,
                file,
                uuid: r.uuid,
                cover_mime: r.cover_mime,
                file_size: r.file_size,
            }
        })
        .collect::<Vec<_>>();
    let shelf_rows: Vec<(String, Option<String>, String, i64)> = sqlx::query_as(&format!(
        "SELECT s.name, s.description, {visibility}, COUNT(sb.book_id) FROM shelves s
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id WHERE s.owner_id = $1 GROUP BY s.id ORDER BY LOWER(s.name)",
        visibility = crate::shelves::VISIBILITY_EXPR,
    ))
    .bind(user_id)
    .fetch_all(&state.db)
    .await?;
    let shelves = shelf_rows
        .into_iter()
        .map(|(name, description, visibility, n)| ExportShelf { name, description, visibility, books: n as usize })
        .collect();
    Ok((books, shelves))
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn to_csv(books: &[ExportBook]) -> String {
    let header = [
        "id", "title", "author", "series", "series_index", "isbn", "identifier", "language", "publisher", "published", "first_published",
        "category", "tags", "shelves", "rating", "progress_percent", "want_to_read", "license", "license_source_url",
        "author_death_year", "added", "file",
    ];
    // A BOM so Excel opens UTF-8 (å, ä, ö) correctly.
    let mut out = String::from("\u{feff}");
    out.push_str(&header.join(","));
    out.push_str("\r\n");
    let o = |v: &Option<String>| v.clone().unwrap_or_default();
    for b in books {
        let fields = [
            b.id.to_string(),
            b.title.clone(),
            o(&b.author),
            o(&b.series),
            b.series_index.map(|v| v.to_string()).unwrap_or_default(),
            o(&b.isbn),
            o(&b.identifier),
            o(&b.language),
            o(&b.publisher),
            o(&b.published),
            b.first_published.map(|v| v.to_string()).unwrap_or_default(),
            o(&b.category),
            b.tags.join("; "),
            b.shelves.join("; "),
            b.rating.map(|v| v.to_string()).unwrap_or_default(),
            b.progress_percent.map(|v| format!("{:.0}", v * 100.0)).unwrap_or_default(),
            if b.want_to_read { "1".into() } else { String::new() },
            o(&b.license),
            o(&b.license_source_url),
            b.author_death_year.map(|v| v.to_string()).unwrap_or_default(),
            b.added.clone(),
            b.file.clone(),
        ];
        out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn to_json(username: &str, books: &[ExportBook], shelves: &[ExportShelf]) -> Value {
    json!({
        "format": "legejo-export-1",
        "exported_at": now_ts(),
        "user": username,
        "books": books,
        "shelves": shelves,
    })
}

#[derive(Deserialize)]
pub struct FormatParam {
    format: Option<String>,
}

/// GET /api/account/metadata?format=csv|json: the metadata alone, synchronously.
pub async fn metadata(State(state): State<AppState>, user: AuthUser, Query(q): Query<FormatParam>) -> Result<Response, Response> {
    let (books, shelves) = collect(&state, user.0.id).await.map_err(internal)?;
    if q.format.as_deref() == Some("json") {
        let body = serde_json::to_string_pretty(&to_json(&user.0.username, &books, &shelves)).map_err(internal)?;
        return Ok((
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8".to_string()),
                (header::CONTENT_DISPOSITION, "attachment; filename=\"legejo.json\"".to_string()),
            ],
            body,
        )
            .into_response());
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"legejo.csv\"".to_string()),
        ],
        to_csv(&books),
    )
        .into_response())
}

// --------------------------------------------------------------- exports

#[derive(Serialize, sqlx::FromRow)]
pub struct ExportRow {
    id: i64,
    status: String,
    total: i64,
    done: i64,
    bytes: i64,
    #[serde(skip)]
    parts: String,
    #[sqlx(skip)]
    #[serde(rename = "parts")]
    parts_json: Value,
    error: Option<String>,
    created_at: String,
    finished_at: Option<String>,
    expires_at: Option<String>,
}

const EXPORT_COLUMNS: &str = "id, status, total, done, bytes, parts, error, created_at, finished_at, expires_at";

async fn latest(state: &AppState, user_id: i64) -> Result<Option<ExportRow>, Response> {
    let mut row: Option<ExportRow> = sqlx::query_as(&format!(
        "SELECT {EXPORT_COLUMNS} FROM exports WHERE user_id = $1 AND status <> 'expired' ORDER BY id DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    if let Some(r) = &mut row {
        r.parts_json = serde_json::from_str(&r.parts).unwrap_or(json!([]));
    }
    Ok(row)
}

fn export_dir(state: &AppState, user_id: i64, export_id: i64) -> std::path::PathBuf {
    state.data_dir.join("exports").join(user_id.to_string()).join(export_id.to_string())
}

/// GET /api/account/export: the current export, or null.
pub async fn status(State(state): State<AppState>, user: AuthUser) -> Result<Json<Option<ExportRow>>, Response> {
    Ok(Json(latest(&state, user.0.id).await?))
}

/// POST /api/account/export: queue a new export, replacing a finished one.
pub async fn start(State(state): State<AppState>, user: AuthUser) -> Result<(StatusCode, Json<Option<ExportRow>>), Response> {
    if let Some(current) = latest(&state, user.0.id).await? {
        if current.status == "queued" || current.status == "running" {
            return Err((StatusCode::CONFLICT, Json(json!({ "error": "an export is already running" }))).into_response());
        }
        remove_export(&state, user.0.id, current.id).await;
    }
    sqlx::query("INSERT INTO exports (user_id, status, created_at) VALUES ($1, 'queued', $2)")
        .bind(user.0.id)
        .bind(now_ts())
        .execute(&state.db)
        .await
        .map_err(internal)?;
    crate::audit::log(&state, crate::audit::by(&user.0), "account.export_started", json!({})).await;
    state.export_wake.notify_one();
    Ok((StatusCode::ACCEPTED, Json(latest(&state, user.0.id).await?)))
}

async fn remove_export(state: &AppState, user_id: i64, export_id: i64) {
    let _ = tokio::fs::remove_dir_all(export_dir(state, user_id, export_id)).await;
    let _ = sqlx::query("UPDATE exports SET status = 'expired', parts = '[]' WHERE id = $1").bind(export_id).execute(&state.db).await;
}

/// DELETE /api/account/export: delete the finished files now.
pub async fn discard(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, Response> {
    let current = latest(&state, user.0.id).await?.ok_or_else(not_found)?;
    if current.status == "running" {
        return Err((StatusCode::CONFLICT, Json(json!({ "error": "the export is running" }))).into_response());
    }
    remove_export(&state, user.0.id, current.id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/account/export/{id}/{part}: one zip part, with Range support.
pub async fn download(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, part)): Path<(i64, String)>,
    req: Request,
) -> Result<Response, Response> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT status, parts FROM exports WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(user.0.id)
            .fetch_optional(&state.db)
            .await
            .map_err(internal)?;
    let (status, parts) = row.ok_or_else(not_found)?;
    let listed = serde_json::from_str::<Vec<Value>>(&parts)
        .unwrap_or_default()
        .iter()
        .any(|p| p.get("name").and_then(Value::as_str) == Some(part.as_str()));
    if status != "done" || !listed || part.contains('/') || part.contains("..") {
        return Err(not_found());
    }
    let path = export_dir(&state, user.0.id, id).join(&part);
    let mut res = tower_http::services::ServeFile::new(&path)
        .oneshot(req)
        .await
        .map_err(internal)?
        .map(axum::body::Body::new);
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{part}\"").parse().map_err(internal)?,
    );
    res.headers_mut().insert(header::CACHE_CONTROL, "private, no-store".parse().map_err(internal)?);
    Ok(res)
}

// ---------------------------------------------------------------- worker

/// Claims and builds queued exports; also takes over stale ones and removes
/// expired files. Runs in every process; the UPDATE decides who gets a job.
pub async fn worker(state: AppState) {
    let me = crate::books::new_uuid();
    loop {
        tokio::select! {
            _ = state.export_wake.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
        }
        // A process that died mid-export leaves a running row without heartbeat.
        let _ = sqlx::query("UPDATE exports SET status = 'queued', worker = NULL WHERE status = 'running' AND heartbeat_at < $1")
            .bind(ts_offset(-STALE_SECS))
            .execute(&state.db)
            .await;
        expire(&state).await;
        run_pending(&state, &me).await;
    }
}

/// Claim and build queued exports until none are left.
pub async fn run_pending(state: &AppState, me: &str) {
    {
        loop {
            let claimed = sqlx::query(
                "UPDATE exports SET status = 'running', worker = $1, heartbeat_at = $2, done = 0, bytes = 0, error = NULL
                 WHERE id = (SELECT id FROM exports WHERE status = 'queued' ORDER BY id LIMIT 1) AND status = 'queued'",
            )
            .bind(me)
            .bind(now_ts())
            .execute(&state.db)
            .await;
            if !matches!(claimed, Ok(r) if r.rows_affected() == 1) {
                break;
            }
            let row: Option<(i64, i64)> =
                sqlx::query_as("SELECT id, user_id FROM exports WHERE status = 'running' AND worker = $1 ORDER BY id LIMIT 1")
                    .bind(me)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();
            let Some((id, user_id)) = row else { break };
            match build(state, id, user_id, me).await {
                Ok(()) => tracing::info!("export {id} done"),
                Err(e) => {
                    tracing::error!("export {id} failed: {e:#}");
                    let _ = tokio::fs::remove_dir_all(export_dir(state, user_id, id)).await;
                    let _ = sqlx::query("UPDATE exports SET status = 'failed', error = $1, finished_at = $2 WHERE id = $3")
                        .bind(e.to_string())
                        .bind(now_ts())
                        .bind(id)
                        .execute(&state.db)
                        .await;
                }
            }
        }
    }
}

async fn expire(state: &AppState) {
    let old: Vec<(i64, i64)> = sqlx::query_as("SELECT id, user_id FROM exports WHERE status = 'done' AND expires_at < $1")
        .bind(now_ts())
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (id, user_id) in old {
        remove_export(state, user_id, id).await;
        tracing::info!("export {id} expired and was removed");
    }
}

/// A zip part being written: temp file renamed into place when finished.
struct Part {
    zip: zip::ZipWriter<std::io::BufWriter<std::fs::File>>,
    tmp: std::path::PathBuf,
    name: String,
    written: u64,
}

fn open_part(dir: &std::path::Path, n: usize) -> anyhow::Result<Part> {
    let name = format!("legejo-part-{n}.zip");
    let tmp = dir.join(format!("{name}.tmp"));
    let file = std::fs::File::create(&tmp)?;
    Ok(Part { zip: zip::ZipWriter::new(std::io::BufWriter::new(file)), tmp, name, written: 0 })
}

fn finish_part(dir: &std::path::Path, part: Part) -> anyhow::Result<(String, u64)> {
    part.zip.finish()?;
    let target = dir.join(&part.name);
    std::fs::rename(&part.tmp, &target)?;
    Ok((part.name, std::fs::metadata(&target)?.len()))
}

fn stored() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true)
}

const README: &str = "Legejo library export\n\
======================\n\n\
books/        Your EPUB files, unmodified (\"Author - Title.epub\"). Each file\n\
              carries its own metadata and can be imported into Calibre or any\n\
              other e-book manager.\n\
covers/       The covers as shown in Legejo, named like the book.\n\
legejo.csv   One row per book: title, author, series, ISBN, tags, shelves,\n\
              rating, reading progress (%), want to read, license, date added\n\
              and file path. Opens in Excel, Numbers or LibreOffice.\n\
legejo.json  The same data plus the shelves, machine-readable.\n\n\
If the export is split into several parts (legejo-part-1.zip, -part-2.zip, ...),\n\
part 1 holds the metadata and the first books; unpack all parts into the same folder.\n";

async fn build(state: &AppState, id: i64, user_id: i64, me: &str) -> anyhow::Result<()> {
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1").bind(user_id).fetch_one(&state.db).await?;
    let (books, shelves) = collect(state, user_id).await?;
    sqlx::query("UPDATE exports SET total = $1 WHERE id = $2").bind(books.len() as i64).bind(id).execute(&state.db).await?;

    let dir = export_dir(state, user_id, id);
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await?;

    // Metadata first, in part 1.
    let csv = to_csv(&books);
    let json = serde_json::to_vec_pretty(&to_json(&username, &books, &shelves))?;
    let d = dir.clone();
    let mut part = tokio::task::spawn_blocking(move || -> anyhow::Result<Part> {
        use std::io::Write;
        let mut part = open_part(&d, 1)?;
        for (name, data) in [("README.txt", README.as_bytes()), ("legejo.csv", csv.as_bytes()), ("legejo.json", &json[..])] {
            part.zip.start_file(name, stored())?;
            part.zip.write_all(data)?;
            part.written += data.len() as u64;
        }
        Ok(part)
    })
    .await??;

    let mut finished: Vec<(String, u64)> = Vec::new();
    let mut n = 1;
    let mut bytes: u64 = 0;
    for (i, book) in books.iter().enumerate() {
        let epub = state.data_dir.join("books").join(format!("{}.epub", book.uuid));
        let cover = book.cover_mime.as_ref().map(|m| {
            let ext = match m.as_str() {
                "image/png" => "png",
                "image/webp" => "webp",
                "image/gif" => "gif",
                _ => "jpg",
            };
            let name = book.file.trim_start_matches("books/").trim_end_matches(".epub");
            (state.data_dir.join("covers").join(&book.uuid), format!("covers/{name}.{ext}"))
        });
        // A new part when this book would push the current one over the cap.
        if part.written > 0 && part.written + book.file_size as u64 > PART_LIMIT {
            let d = dir.clone();
            finished.push(tokio::task::spawn_blocking(move || finish_part(&d, part)).await??);
            n += 1;
            let d = dir.clone();
            part = tokio::task::spawn_blocking(move || open_part(&d, n)).await??;
        }
        let file_name = book.file.clone();
        let (p, added) = tokio::task::spawn_blocking(move || -> anyhow::Result<(Part, u64)> {
            let mut part = part;
            let mut added = 0;
            if let Ok(mut f) = std::fs::File::open(&epub) {
                part.zip.start_file(file_name, stored())?;
                added += std::io::copy(&mut f, &mut part.zip)?;
            }
            if let Some((src, name)) = cover {
                if let Ok(mut f) = std::fs::File::open(&src) {
                    part.zip.start_file(name, stored())?;
                    added += std::io::copy(&mut f, &mut part.zip)?;
                }
            }
            part.written += added;
            Ok((part, added))
        })
        .await??;
        part = p;
        bytes += added;
        // Progress (and heartbeat) every few books; also stop if it was taken over.
        if i % 5 == 4 || i + 1 == books.len() {
            let r = sqlx::query("UPDATE exports SET done = $1, bytes = $2, heartbeat_at = $3 WHERE id = $4 AND worker = $5")
                .bind((i + 1) as i64)
                .bind(bytes as i64)
                .bind(now_ts())
                .bind(id)
                .bind(me)
                .execute(&state.db)
                .await?;
            if r.rows_affected() == 0 {
                anyhow::bail!("the export was taken over or removed");
            }
        }
    }
    let d = dir.clone();
    finished.push(tokio::task::spawn_blocking(move || finish_part(&d, part)).await??);

    let parts: Vec<Value> = finished.iter().map(|(name, size)| json!({ "name": name, "size": size })).collect();
    sqlx::query(
        "UPDATE exports SET status = 'done', done = total, bytes = $1, parts = $2, finished_at = $3, expires_at = $4
         WHERE id = $5 AND worker = $6",
    )
    .bind(bytes as i64)
    .bind(serde_json::to_string(&parts)?)
    .bind(now_ts())
    .bind(ts_offset(EXPORT_DAYS * 24 * 3600))
    .bind(id)
    .bind(me)
    .execute(&state.db)
    .await?;
    notify(state, user_id, &username).await;
    Ok(())
}

/// Mail the user that the export is ready, if they have an address and mail is configured.
async fn notify(state: &AppState, user_id: i64, username: &str) {
    let email: Option<String> =
        sqlx::query_scalar("SELECT email FROM users WHERE id = $1").bind(user_id).fetch_optional(&state.db).await.ok().flatten();
    let (Some(email), Some(_)) = (email, &state.mail) else { return };
    let link = match &state.fed.config {
        Some(c) => format!("{}/account", c.base),
        None => "/account".into(),
    };
    let html = format!(
        "<p>Hello {username},</p><p>The export of your Legejo library is ready. Download it from your account page:</p>\
         <p><a href=\"{link}\">{link}</a></p><p>The files are kept for {EXPORT_DAYS} days.</p>"
    );
    if let Err(e) = crate::register::send_mail(state, &email, "Your Legejo export is ready", &html).await {
        tracing::warn!("export notification mail failed: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_what_needs_it() {
        assert_eq!(csv_field("Röda rummet"), "Röda rummet");
        assert_eq!(csv_field("Sjöwall, Maj"), "\"Sjöwall, Maj\"");
        assert_eq!(csv_field("Han sa \"hej\""), "\"Han sa \"\"hej\"\"\"");
        assert_eq!(file_base(Some("Selma Lagerlöf"), "Nils Holgerssons / resa"), "Selma Lagerlöf - Nils Holgerssons _ resa");
        assert_eq!(file_base(None, "..."), "book");
    }
}
