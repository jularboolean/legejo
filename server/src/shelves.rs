use crate::auth::AuthUser;
use crate::books::{Book, BOOK_COLUMNS_B};
use crate::db::{now_ts, DbFlag};
use crate::progress::{self, PROGRESS_JOINS};
use crate::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub struct Shelf {
    pub id: i64,
    pub name: String,
    /// private | instance | federated.
    pub visibility: String,
    /// visibility <> 'private'; kept for older clients.
    pub is_public: DbFlag,
    pub description: Option<String>,
    pub has_cover: DbFlag,
    pub book_count: i64,
    /// The federation handle's local part, set when the shelf first federates.
    pub ap_slug: Option<String>,
    /// Remote actors following the shelf.
    pub followers: i64,
    /// "@slug@host", only while the shelf federates.
    #[sqlx(skip)]
    pub handle: Option<String>,
    /// Proposal for ap_slug: "<user>-<shelf>".
    #[sqlx(skip)]
    pub suggested_slug: String,
}

#[derive(Serialize)]
pub struct ShelfDetail {
    #[serde(flatten)]
    pub shelf: Shelf,
    pub books: Vec<Book>,
}

const SHELF_COLUMNS: &str = "s.id, s.name, s.visibility, s.is_public, s.description, \
     CAST(CASE WHEN s.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover, \
     COUNT(sb.book_id) AS book_count, s.ap_slug, \
     (SELECT COUNT(*) FROM ap_followers f WHERE f.shelf_id = s.id) AS followers";

const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;

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

fn conflict() -> Response {
    (StatusCode::CONFLICT, Json(serde_json::json!({ "error": "a shelf with that name already exists" })))
        .into_response()
}

/// Fill in the federation fields the query can't.
async fn decorate(state: &AppState, user_id: i64, shelves: &mut [Shelf]) {
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or_default();
    let host = state.fed.config.as_ref().map(|c| c.host.clone());
    for shelf in shelves {
        shelf.suggested_slug = crate::fed::slugify(&format!("{username}-{}", shelf.name));
        shelf.handle = match (&host, &shelf.ap_slug) {
            (Some(host), Some(slug)) if shelf.visibility == "federated" => Some(format!("@{slug}@{host}")),
            _ => None,
        };
    }
}

/// The books on a shelf that may not federate, with the reason for each.
pub(crate) async fn blocking_books(state: &AppState, shelf_id: i64) -> Result<Vec<serde_json::Value>, Response> {
    let books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b JOIN shelf_books sb ON sb.book_id = b.id WHERE sb.shelf_id = $1"
    ))
    .bind(shelf_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(books
        .iter()
        .filter_map(|b| match crate::license::federable(&b.license_facts(), crate::license::current_year()) {
            Ok(()) => None,
            Err(reason) => Some(serde_json::json!({ "id": b.id, "title": b.title, "reason": reason })),
        })
        .collect())
}

pub(crate) fn not_federable(blocking: Vec<serde_json::Value>) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({ "error": "books not federable", "blocking": blocking })),
    )
        .into_response()
}

async fn fetch_shelf(state: &AppState, user_id: i64, shelf_id: i64) -> Result<Shelf, Response> {
    let shelf: Option<Shelf> = sqlx::query_as(&format!(
        "SELECT {SHELF_COLUMNS} FROM shelves s
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE s.id = $1 AND s.owner_id = $2
         GROUP BY s.id"
    ))
    .bind(shelf_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    let mut shelves: Vec<Shelf> = shelf.into_iter().collect();
    decorate(state, user_id, &mut shelves).await;
    shelves.pop().ok_or_else(not_found)
}

pub async fn list(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<Shelf>>, Response> {
    let mut shelves: Vec<Shelf> = sqlx::query_as(&format!(
        "SELECT {SHELF_COLUMNS} FROM shelves s
         LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
         WHERE s.owner_id = $1
         GROUP BY s.id
         ORDER BY LOWER(s.name)"
    ))
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    decorate(&state, user.0.id, &mut shelves).await;
    Ok(Json(shelves))
}

pub async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<ShelfDetail>, Response> {
    let shelf = fetch_shelf(&state, user.0.id, id).await?;
    let books: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B}, {percent} AS progress_percent FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         {PROGRESS_JOINS}
         WHERE b.owner_id = $1 AND sb.shelf_id = $2
         ORDER BY sb.added_at DESC, b.id DESC",
        percent = progress::progress_percent(state.backend),
    ))
    .bind(user.0.id)
    .bind(shelf.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| internal(e.into()))?;
    Ok(Json(ShelfDetail { shelf, books }))
}

#[derive(Deserialize)]
pub struct CreateShelf {
    name: String,
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateShelf>,
) -> Result<(StatusCode, Json<Shelf>), Response> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(unprocessable("name must not be empty"));
    }

    let result = sqlx::query_scalar::<_, i64>(
        "INSERT INTO shelves (owner_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(user.0.id)
    .bind(name)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(id) => {
            let shelf = fetch_shelf(&state, user.0.id, id).await?;
            crate::audit::log(&state, crate::audit::by(&user.0), "shelf.created", serde_json::json!({ "shelf_id": id, "name": shelf.name })).await;
            Ok((StatusCode::CREATED, Json(shelf)))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(conflict()),
        Err(e) => Err(internal(e.into())),
    }
}

/// Who sees a shelf. `Instance` is what "public" has always meant.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Instance,
    Federated,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Instance => "instance",
            Visibility::Federated => "federated",
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateShelf {
    name: String,
    description: Option<String>,
    /// Preferred; when absent, the older boolean decides.
    visibility: Option<Visibility>,
    #[serde(default)]
    is_public: bool,
    /// The handle, the first time the shelf federates.
    ap_slug: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateShelf>,
) -> Result<Json<Shelf>, Response> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(unprocessable("name must not be empty"));
    }
    let description = req.description.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let visibility = req.visibility.unwrap_or(if req.is_public { Visibility::Instance } else { Visibility::Private });
    let mut new_slug: Option<String> = None;
    if visibility == Visibility::Federated {
        if crate::fed::active(&state).await.is_none() {
            return Err(unprocessable("federation is off"));
        }
        let current = fetch_shelf(&state, user.0.id, id).await?;
        let wanted = req.ap_slug.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
        let slug = match (&current.ap_slug, wanted) {
            // Remote servers cache actor ids: a handle never changes once used.
            (Some(existing), Some(w)) if w != *existing => return Err(unprocessable("handle cannot change")),
            (Some(existing), _) => existing.clone(),
            (None, Some(w)) => w,
            (None, None) => current.suggested_slug.clone(),
        };
        if !crate::fed::valid_slug(&slug) {
            return Err(unprocessable("invalid handle"));
        }
        let taken: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelves WHERE lower(ap_slug) = lower($1) AND id <> $2")
            .bind(&slug)
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
        if taken > 0 {
            return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "handle taken" }))).into_response());
        }
        let blocking = blocking_books(&state, id).await?;
        if !blocking.is_empty() {
            return Err(not_federable(blocking));
        }
        new_slug = Some(slug);
    }

    // is_public follows visibility, so an older image reads the same shelves
    // as public after a rollback.
    let result = sqlx::query(
        "UPDATE shelves SET name = $1, description = $2, visibility = $3, is_public = $4, updated_at = $5,
             ap_slug = COALESCE(ap_slug, $8)
         WHERE id = $6 AND owner_id = $7",
    )
    .bind(name)
    .bind(&description)
    .bind(visibility.as_str())
    .bind(DbFlag::from(visibility != Visibility::Private))
    .bind(now_ts())
    .bind(id)
    .bind(user.0.id)
    .bind(&new_slug)
    .execute(&state.db)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => Err(not_found()),
        Ok(_) => {
            state.fed.wake.notify_one();
            let shelf = fetch_shelf(&state, user.0.id, id).await?;
            crate::audit::log(
                &state,
                crate::audit::by(&user.0),
                "shelf.edited",
                serde_json::json!({ "shelf_id": id, "name": shelf.name, "visibility": shelf.visibility, "handle": shelf.handle }),
            )
            .await;
            Ok(Json(shelf))
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(conflict()),
        Err(e) => Err(internal(e.into())),
    }
}

pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    // Retract a federated shelf before deleting it: the follower rows needed
    // to address the Delete are removed along with the shelf.
    let existing = fetch_shelf(&state, user.0.id, id).await.ok();
    if let Some(shelf) = &existing {
        if let (Some(c), Some(slug)) = (crate::fed::active(&state).await, &shelf.ap_slug) {
            if let Err(e) = crate::fed::reconcile::retract_shelf(&state, &c, id, slug).await {
                tracing::error!("fed: could not retract shelf {slug}: {e:#}");
            }
        }
    }
    let result = sqlx::query("DELETE FROM shelves WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    // Tombstone so Kobo devices remove the matching collection on next sync.
    sqlx::query(
        "INSERT INTO kobo_deleted_shelves (shelf_id, owner_id, deleted_at) VALUES ($1, $2, $3)
         ON CONFLICT (shelf_id) DO UPDATE SET owner_id = excluded.owner_id, deleted_at = excluded.deleted_at",
    )
    .bind(id)
    .bind(user.0.id)
    .bind(now_ts())
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    let _ = tokio::fs::remove_file(shelf_cover_path(&state, id)).await;
    crate::audit::log(
        &state,
        crate::audit::by(&user.0),
        "shelf.deleted",
        serde_json::json!({ "shelf_id": id, "name": existing.map(|s| s.name) }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

fn shelf_cover_path(state: &AppState, id: i64) -> std::path::PathBuf {
    state.data_dir.join("shelf_covers").join(id.to_string())
}

pub async fn upload_cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Shelf>, Response> {
    // Ensure the shelf exists and is ours before accepting data.
    fetch_shelf(&state, user.0.id, id).await?;

    let mut uploaded: Option<(Vec<u8>, String)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| internal(e.into()))? {
        let mime = field.content_type().unwrap_or("").to_string();
        if !mime.starts_with("image/") {
            continue;
        }
        let bytes = field.bytes().await.map_err(|e| internal(e.into()))?;
        if bytes.len() > MAX_COVER_BYTES {
            return Err(unprocessable("image too large (max 10 MB)"));
        }
        uploaded = Some((bytes.to_vec(), mime));
        break;
    }
    let (bytes, mime) = uploaded.ok_or_else(|| unprocessable("no image file in upload"))?;

    tokio::fs::create_dir_all(state.data_dir.join("shelf_covers"))
        .await
        .map_err(|e| internal(e.into()))?;
    tokio::fs::write(shelf_cover_path(&state, id), &bytes)
        .await
        .map_err(|e| internal(e.into()))?;

    sqlx::query("UPDATE shelves SET cover_mime = $1 WHERE id = $2 AND owner_id = $3")
        .bind(&mime)
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;

    Ok(Json(fetch_shelf(&state, user.0.id, id).await?))
}

pub async fn delete_cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<Shelf>, Response> {
    let result = sqlx::query("UPDATE shelves SET cover_mime = NULL WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(|e| internal(e.into()))?;
    if result.rows_affected() == 0 {
        return Err(not_found());
    }
    let _ = tokio::fs::remove_file(shelf_cover_path(&state, id)).await;
    Ok(Json(fetch_shelf(&state, user.0.id, id).await?))
}

pub async fn cover(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Response, Response> {
    let mime: Option<Option<String>> =
        sqlx::query_scalar("SELECT cover_mime FROM shelves WHERE id = $1 AND owner_id = $2")
            .bind(id)
            .bind(user.0.id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| internal(e.into()))?;
    let mime = mime.flatten().ok_or_else(not_found)?;
    let data = tokio::fs::read(shelf_cover_path(&state, id)).await.map_err(|_| not_found())?;
    Ok((
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, no-cache".into())],
        data,
    )
        .into_response())
}
