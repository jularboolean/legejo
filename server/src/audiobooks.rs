//! Audiobooks: a set of audio files with metadata. Legejo keeps the files and
//! hands them out as a podcast feed, one episode per file, so that any podcast
//! app plays them and keeps the listening position. A feed is reached through
//! a secret key in its address, since podcast apps cannot log in.
//!
//! Off until an admin turns it on (the setting `audiobooks_enabled`).

use crate::auth::AuthUser;
use crate::books::new_uuid;
use crate::db::{now_ts, DbFlag};
use crate::AppState;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;

pub async fn enabled(state: &AppState) -> bool {
    crate::admin::setting_bool(state, "audiobooks_enabled", false).await.unwrap_or(false)
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

async fn on(state: &AppState) -> Result<(), Response> {
    if enabled(state).await {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// The audio formats taken in, by file extension: (extension, media type).
/// These are the two that every podcast app plays.
fn audio_type(filename: &str) -> Option<(&'static str, &'static str)> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp3" => Some(("mp3", "audio/mpeg")),
        "m4a" => Some(("m4a", "audio/mp4")),
        "m4b" => Some(("m4b", "audio/mp4")),
        _ => None,
    }
}

fn audio_dir(state: &AppState, uuid: &str) -> std::path::PathBuf {
    state.data_dir.join("audio").join(uuid)
}

fn cover_path(state: &AppState, uuid: &str) -> std::path::PathBuf {
    state.data_dir.join("audio_covers").join(uuid)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Audiobook {
    pub id: i64,
    #[serde(skip)]
    pub uuid: String,
    pub title: String,
    pub author: Option<String>,
    pub narrator: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub has_cover: DbFlag,
    #[serde(skip)]
    pub feed_key: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    /// How many files, their length in seconds and their size in bytes.
    pub parts: i64,
    pub seconds: i64,
    pub bytes: i64,
}

const COLUMNS: &str = "a.id, a.uuid, a.title, a.author, a.narrator, a.language, a.description, \
     CAST(CASE WHEN a.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover, \
     a.feed_key, a.created_at, a.updated_at, \
     (SELECT COUNT(*) FROM audiobook_files f WHERE f.audiobook_id = a.id) AS parts, \
     CAST(COALESCE((SELECT SUM(f.seconds) FROM audiobook_files f WHERE f.audiobook_id = a.id), 0) AS BIGINT) AS seconds, \
     CAST(COALESCE((SELECT SUM(f.bytes) FROM audiobook_files f WHERE f.audiobook_id = a.id), 0) AS BIGINT) AS bytes";

#[derive(Serialize, sqlx::FromRow, Clone)]
pub struct Part {
    pub id: i64,
    #[serde(skip)]
    pub uuid: String,
    pub position: i64,
    /// From the file's tags; absent when it has none.
    pub title: Option<String>,
    pub filename: String,
    #[serde(skip)]
    pub mime: String,
    pub seconds: i64,
    pub bytes: i64,
}

const PART_COLUMNS: &str = "id, uuid, position, title, filename, mime, seconds, bytes";

#[derive(Serialize)]
pub struct Detail {
    #[serde(flatten)]
    pub book: Audiobook,
    pub files: Vec<Part>,
    /// The podcast feed: the address to give a podcast app.
    pub feed_url: String,
}

async fn fetch(state: &AppState, user_id: i64, id: i64) -> Result<Audiobook, Response> {
    let book: Option<Audiobook> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM audiobooks a WHERE a.id = $1 AND a.owner_id = $2"))
            .bind(id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    book.ok_or_else(not_found)
}

async fn parts(state: &AppState, audiobook_id: i64) -> Result<Vec<Part>, Response> {
    sqlx::query_as(&format!(
        "SELECT {PART_COLUMNS} FROM audiobook_files WHERE audiobook_id = $1 ORDER BY position, id"
    ))
    .bind(audiobook_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))
}

async fn detail(state: &AppState, headers: &HeaderMap, user_id: i64, id: i64) -> Result<Json<Detail>, Response> {
    let book = fetch(state, user_id, id).await?;
    let files = parts(state, book.id).await?;
    let feed_url = format!("{}/podcast/{}/feed.xml", crate::invite::base(state, headers), book.feed_key);
    Ok(Json(Detail { book, files, feed_url }))
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<Audiobook>>, Response> {
    on(&state).await?;
    let books: Vec<Audiobook> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM audiobooks a WHERE a.owner_id = $1 ORDER BY a.created_at DESC, a.id DESC"
    ))
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(books))
}

#[derive(Deserialize)]
pub struct Create {
    #[serde(default)]
    title: String,
}

/// An audiobook starts empty; its files are added one request at a time, so
/// that each has its own progress and its own size limit.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Json(req): Json<Create>,
) -> Result<(StatusCode, Json<Detail>), Response> {
    on(&state).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO audiobooks (uuid, owner_id, title, feed_key) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(new_uuid())
    .bind(user.0.id)
    .bind(req.title.trim())
    .bind(new_uuid())
    .fetch_one(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok((StatusCode::CREATED, detail(&state, &headers, user.0.id, id).await?))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Detail>, Response> {
    on(&state).await?;
    detail(&state, &headers, user.0.id, id).await
}

#[derive(Deserialize)]
pub struct Update {
    title: String,
    author: Option<String>,
    narrator: Option<String>,
    language: Option<String>,
    description: Option<String>,
}

fn clean(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(req): Json<Update>,
) -> Result<Json<Detail>, Response> {
    on(&state).await?;
    let title = req.title.trim();
    if title.is_empty() {
        return Err(unprocessable("title must not be empty"));
    }
    let result = sqlx::query(
        "UPDATE audiobooks SET title = $1, author = $2, narrator = $3, language = $4, description = $5, updated_at = $6
         WHERE id = $7 AND owner_id = $8",
    )
    .bind(title)
    .bind(clean(req.author))
    .bind(clean(req.narrator))
    .bind(clean(req.language))
    .bind(clean(req.description))
    .bind(now_ts())
    .bind(id)
    .bind(user.0.id)
    .execute(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    detail(&state, &headers, user.0.id, id).await
}

/// Remove an audiobook's files from disk. The rows go with the audiobook.
pub(crate) async fn remove_files(state: &AppState, uuid: &str) {
    let _ = tokio::fs::remove_dir_all(audio_dir(state, uuid)).await;
    let _ = tokio::fs::remove_file(cover_path(state, uuid)).await;
}

pub async fn delete(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<StatusCode, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    sqlx::query("DELETE FROM audiobooks WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    remove_files(&state, &book.uuid).await;
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "audiobook.deleted",
        serde_json::json!({ "audiobook_id": id, "title": book.title }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// What a file says about itself.
#[derive(Default)]
struct Tags {
    seconds: i64,
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    cover: Option<(Vec<u8>, String)>,
}

fn read_tags(path: &std::path::Path) -> Tags {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;
    let Ok(file) = lofty::read_from_path(path) else { return Tags::default() };
    let mut tags = Tags { seconds: file.properties().duration().as_secs() as i64, ..Tags::default() };
    if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
        let text = |v: Option<std::borrow::Cow<str>>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        tags.title = text(tag.title());
        tags.album = text(tag.album());
        tags.artist = text(tag.artist());
        tags.cover = tag.pictures().first().and_then(|p| {
            let mime = p.mime_type()?.as_str().to_string();
            (mime.starts_with("image/") && p.data().len() <= MAX_COVER_BYTES).then(|| (p.data().to_vec(), mime))
        });
    }
    tags
}

/// A title for an audiobook whose files say nothing: the file name without
/// its counter and technical suffixes.
fn title_from_filename(filename: &str) -> String {
    let stem = filename.rsplit_once('.').map_or(filename, |(s, _)| s);
    let words: Vec<&str> = stem
        .split(|c: char| c == '_' || c == '-' || c.is_whitespace())
        .filter(|w| !w.is_empty() && !w.chars().all(|c| c.is_ascii_digit()) && !w.to_ascii_lowercase().ends_with("kb"))
        .collect();
    if words.is_empty() {
        stem.to_string()
    } else {
        words.join(" ")
    }
}

#[derive(Serialize)]
pub struct Added {
    #[serde(flatten)]
    detail: Detail,
    /// Files left out, with why.
    errors: Vec<String>,
}

/// Add audio files to an audiobook, in the order they arrive. The first
/// file's tags fill in what the audiobook does not have yet.
pub async fn add_files(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Added>, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    let dir = audio_dir(&state, &book.uuid);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| internal(e.into()))?;
    let mut errors = Vec::new();

    while let Some(mut field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let filename = field.file_name().unwrap_or("audio").to_string();
        let Some((ext, mime)) = audio_type(&filename) else {
            errors.push(format!("{filename}: not an audio format that podcast apps play (mp3, m4a, m4b)"));
            continue;
        };
        // Straight to disk: an audio file is too large to hold in memory.
        let uuid = new_uuid();
        let path = dir.join(format!("{uuid}.{ext}"));
        let mut file = tokio::fs::File::create(&path).await.map_err(|e| internal(e.into()))?;
        let mut bytes: i64 = 0;
        let mut failed = None;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    bytes += chunk.len() as i64;
                    if let Err(e) = file.write_all(&chunk).await {
                        failed = Some(e.to_string());
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    failed = Some(e.to_string());
                    break;
                }
            }
        }
        let _ = file.flush().await;
        drop(file);
        if failed.is_some() || bytes == 0 {
            let _ = tokio::fs::remove_file(&path).await;
            errors.push(format!("{filename}: could not read the upload"));
            continue;
        }

        let probe = path.clone();
        let tags = tokio::task::spawn_blocking(move || read_tags(&probe)).await.unwrap_or_default();
        let position: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(position), 0) + 1 FROM audiobook_files WHERE audiobook_id = $1")
                .bind(book.id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
        sqlx::query(
            "INSERT INTO audiobook_files (audiobook_id, uuid, position, title, filename, mime, seconds, bytes)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(book.id)
        .bind(&uuid)
        .bind(position)
        .bind(&tags.title)
        .bind(&filename)
        .bind(mime)
        .bind(tags.seconds)
        .bind(bytes)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;

        // Fill in what is missing; never replace what is there.
        sqlx::query(
            "UPDATE audiobooks SET
                 title = CASE WHEN title = '' THEN $1 ELSE title END,
                 author = COALESCE(author, $2),
                 updated_at = $3
             WHERE id = $4",
        )
        .bind(tags.album.clone().unwrap_or_else(|| title_from_filename(&filename)))
        .bind(&tags.artist)
        .bind(now_ts())
        .bind(book.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
        if let Some((data, cover_mime)) = tags.cover {
            let has: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM audiobooks WHERE id = $1")
                .bind(book.id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| internal(e.into()))?;
            if has.is_none() {
                store_cover(&state, book.id, &book.uuid, &data, &cover_mime).await?;
            }
        }
    }

    if book.parts == 0 {
        crate::audit::log(
            &state,
            crate::audit::by(&user.0),
            "audiobook.added",
            serde_json::json!({ "audiobook_id": book.id }),
        )
        .await;
    }
    let Json(detail) = detail(&state, &headers, user.0.id, id).await?;
    Ok(Json(Added { detail, errors }))
}

pub async fn delete_file(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((id, file_id)): Path<(i64, i64)>,
) -> Result<Json<Detail>, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    let part = parts(&state, book.id).await?.into_iter().find(|p| p.id == file_id).ok_or_else(not_found)?;
    sqlx::query("DELETE FROM audiobook_files WHERE id = $1 AND audiobook_id = $2")
        .bind(file_id)
        .bind(book.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let _ = tokio::fs::remove_file(part_path(&state, &book.uuid, &part)).await;
    detail(&state, &headers, user.0.id, id).await
}

fn part_path(state: &AppState, book_uuid: &str, part: &Part) -> std::path::PathBuf {
    let ext = audio_type(&part.filename).map_or("mp3", |(ext, _)| ext);
    audio_dir(state, book_uuid).join(format!("{}.{ext}", part.uuid))
}

/// A new feed key: the old address stops working at once.
pub async fn new_feed_key(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Detail>, Response> {
    on(&state).await?;
    let result = sqlx::query("UPDATE audiobooks SET feed_key = $1 WHERE id = $2 AND owner_id = $3")
        .bind(new_uuid())
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    detail(&state, &headers, user.0.id, id).await
}

// ---- Covers ------------------------------------------------------------------

async fn store_cover(state: &AppState, id: i64, uuid: &str, data: &[u8], mime: &str) -> Result<(), Response> {
    tokio::fs::create_dir_all(state.data_dir.join("audio_covers")).await.map_err(|e| internal(e.into()))?;
    tokio::fs::write(cover_path(state, uuid), data).await.map_err(|e| internal(e.into()))?;
    sqlx::query("UPDATE audiobooks SET cover_mime = $1, updated_at = $2 WHERE id = $3")
        .bind(mime)
        .bind(now_ts())
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    Ok(())
}

pub async fn upload_cover(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Detail>, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    while let Some(field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let mime = field.content_type().unwrap_or("").to_string();
        if !mime.starts_with("image/") {
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| internal(e.into()))?;
        if bytes.len() > MAX_COVER_BYTES {
            return Err(unprocessable("image too large (max 10 MB)"));
        }
        store_cover(&state, book.id, &book.uuid, &bytes, &mime).await?;
        return detail(&state, &headers, user.0.id, id).await;
    }
    Err(unprocessable("no image file in upload"))
}

async fn cover_response(state: &AppState, uuid: &str) -> Result<Response, Response> {
    let mime: Option<String> = sqlx::query_scalar("SELECT cover_mime FROM audiobooks WHERE uuid = $1")
        .bind(uuid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?
        .flatten();
    let mime = mime.ok_or_else(not_found)?;
    let data = tokio::fs::read(cover_path(state, uuid)).await.map_err(|_| not_found())?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, no-cache".into())], data).into_response())
}

pub async fn cover(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<Response, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    cover_response(&state, &book.uuid).await
}

// ---- Audio, with partial requests ---------------------------------------------

/// The part of a file a Range header asks for: (first byte, last byte).
/// None for a range that cannot be met.
fn byte_range(header: Option<&str>, size: u64) -> Option<Option<(u64, u64)>> {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else { return Some(None) };
    // One range only; a request for several gets the whole file.
    if spec.contains(',') {
        return Some(None);
    }
    let (from, to) = spec.split_once('-')?;
    let last = size.checked_sub(1)?;
    let range = match (from.trim().parse::<u64>().ok(), to.trim().parse::<u64>().ok()) {
        (Some(start), end) => (start, end.map_or(last, |e| e.min(last))),
        // "-500": the last 500 bytes.
        (None, Some(suffix)) if suffix > 0 => (size.saturating_sub(suffix), last),
        _ => return None,
    };
    (range.0 <= range.1 && range.0 < size).then_some(Some(range))
}

/// Send an audio file, whole or the part asked for. Podcast apps need the
/// latter to resume and to seek without fetching everything before.
async fn send_audio(path: &std::path::Path, mime: &str, headers: &HeaderMap) -> Result<Response, Response> {
    let mut file = tokio::fs::File::open(path).await.map_err(|_| not_found())?;
    let size = file.metadata().await.map_err(|e| internal(e.into()))?.len();
    let wanted = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let Some(range) = byte_range(wanted, size) else {
        return Ok((StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{size}"))]).into_response());
    };
    let (start, end) = range.unwrap_or((0, size.saturating_sub(1)));
    let length = if size == 0 { 0 } else { end - start + 1 };
    file.seek(std::io::SeekFrom::Start(start)).await.map_err(|e| internal(e.into()))?;
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file.take(length)));
    let mut response = Response::builder()
        .status(if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "private, no-store");
    if range.is_some() {
        response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    response.body(body).map_err(|e| internal(e.into()))
}

/// One file, for listening in the web interface.
pub async fn audio(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path((id, file_id)): Path<(i64, i64)>,
) -> Result<Response, Response> {
    on(&state).await?;
    let book = fetch(&state, user.0.id, id).await?;
    let part = parts(&state, book.id).await?.into_iter().find(|p| p.id == file_id).ok_or_else(not_found)?;
    send_audio(&part_path(&state, &book.uuid, &part), &part.mime, &headers).await
}

// ---- The podcast feed -------------------------------------------------------

/// Routes that a podcast app uses. No session: the key in the path is the
/// way in.
pub fn podcast_router() -> Router<AppState> {
    Router::new()
        .route("/{key}/feed.xml", get(feed))
        .route("/{key}/cover", get(feed_cover))
        .route("/{key}/{file}", get(feed_audio))
        .fallback(|| async { StatusCode::NOT_FOUND })
}

async fn by_key(state: &AppState, key: &str) -> Result<Audiobook, Response> {
    if !enabled(state).await {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let book: Option<Audiobook> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM audiobooks a WHERE a.feed_key = $1"))
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    book.ok_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn clock(seconds: i64) -> String {
    format!("{}:{:02}:{:02}", seconds / 3600, seconds % 3600 / 60, seconds % 60)
}

/// Podcast apps list the newest episode first. An audiobook is heard from
/// the start, so the feed says it is a serial, numbers the episodes, and
/// dates them one day apart from a fixed day, the first part the oldest.
fn feed_xml(base: &str, book: &Audiobook, files: &[Part]) -> String {
    let root = format!("{base}/podcast/{}", book.feed_key);
    let epoch = time::macros::datetime!(2020-01-01 12:00 UTC);
    let mut items = String::new();
    for (i, part) in files.iter().enumerate() {
        let n = i as i64 + 1;
        let ext = audio_type(&part.filename).map_or("mp3", |(ext, _)| ext);
        let date = (epoch + time::Duration::days(i as i64))
            .format(&time::format_description::well_known::Rfc2822)
            .unwrap_or_default();
        let title = part.title.clone().unwrap_or_else(|| format!("Part {n}"));
        items.push_str(&format!(
            "    <item>\n      <title>{title}</title>\n      <guid isPermaLink=\"false\">{guid}</guid>\n      \
             <pubDate>{date}</pubDate>\n      <enclosure url=\"{root}/{uuid}.{ext}\" length=\"{bytes}\" type=\"{mime}\"/>\n      \
             <itunes:duration>{duration}</itunes:duration>\n      <itunes:episode>{n}</itunes:episode>\n      \
             <itunes:episodeType>full</itunes:episodeType>\n    </item>\n",
            title = xml(&title),
            guid = part.uuid,
            uuid = part.uuid,
            bytes = part.bytes,
            mime = part.mime,
            duration = clock(part.seconds),
        ));
    }
    let image = if book.has_cover.as_bool() {
        format!("    <itunes:image href=\"{root}/cover\"/>\n")
    } else {
        String::new()
    };
    let author = book.author.as_deref().map(|a| format!("    <itunes:author>{}</itunes:author>\n", xml(a))).unwrap_or_default();
    let description = [
        book.description.clone(),
        book.narrator.as_deref().map(|n| format!("Read by {n}.")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\">\n  <channel>\n    \
         <title>{title}</title>\n    <link>{base}</link>\n    <language>{language}</language>\n    \
         <description>{description}</description>\n{author}    <itunes:type>serial</itunes:type>\n    \
         <itunes:block>Yes</itunes:block>\n    <itunes:explicit>false</itunes:explicit>\n{image}{items}  </channel>\n</rss>\n",
        title = xml(&book.title),
        language = xml(book.language.as_deref().unwrap_or("en")),
        description = xml(&description),
    )
}

async fn feed(State(state): State<AppState>, headers: HeaderMap, Path(key): Path<String>) -> Result<Response, Response> {
    let book = by_key(&state, &key).await?;
    let files = parts(&state, book.id).await?;
    let body = feed_xml(&crate::invite::base(&state, &headers), &book, &files);
    Ok((
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"), (header::CACHE_CONTROL, "private, no-store")],
        body,
    )
        .into_response())
}

async fn feed_cover(State(state): State<AppState>, Path(key): Path<String>) -> Result<Response, Response> {
    let book = by_key(&state, &key).await?;
    cover_response(&state, &book.uuid).await
}

async fn feed_audio(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((key, file)): Path<(String, String)>,
) -> Result<Response, Response> {
    let book = by_key(&state, &key).await?;
    let uuid = file.split('.').next().unwrap_or("");
    let part = parts(&state, book.id)
        .await?
        .into_iter()
        .find(|p| p.uuid == uuid)
        .ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    send_audio(&part_path(&state, &book.uuid, &part), &part.mime, &headers).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(byte_range(None, 100), Some(None));
        assert_eq!(byte_range(Some("bytes=0-9"), 100), Some(Some((0, 9))));
        assert_eq!(byte_range(Some("bytes=90-"), 100), Some(Some((90, 99))));
        assert_eq!(byte_range(Some("bytes=90-500"), 100), Some(Some((90, 99))));
        assert_eq!(byte_range(Some("bytes=-10"), 100), Some(Some((90, 99))));
        assert_eq!(byte_range(Some("bytes=100-"), 100), None);
        assert_eq!(byte_range(Some("bytes=9-0"), 100), None);
        assert_eq!(byte_range(Some("bytes=0-1,5-6"), 100), Some(None));
        assert_eq!(byte_range(Some("bytes=0-"), 0), None);
    }

    #[test]
    fn titles_and_types() {
        assert_eq!(title_from_filename("signofthecross_01_gaume_64kb.mp3"), "signofthecross gaume");
        assert_eq!(title_from_filename("01.mp3"), "01");
        assert_eq!(audio_type("Chapter 1.M4B"), Some(("m4b", "audio/mp4")));
        assert_eq!(audio_type("notes.txt"), None);
        assert_eq!(clock(3725), "1:02:05");
    }
}
