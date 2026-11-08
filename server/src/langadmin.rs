//! Admin → Languages: the language packs an instance has and the ones the
//! catalogue offers (langcatalogue.rs), a pack installed from the catalogue
//! or imported from a zip, and a pack removed again. Esperanto needs no
//! pack: it has ESPDIC, in the binary.
//!
//! A zip holds a text pack (`pack.json`, `lemmas.tsv`, `forms.tsv`, and
//! optionally `README.txt`) at its root or under one top folder. It is
//! unpacked into a hidden directory next to the packs, loaded there as
//! `Packs` would load it, and only then put in the place of the pack there
//! is, by renaming directories: a broken pack never replaces a working one,
//! and a process that reads the directory sees the old pack or the new one.
//!
//! The packs live in `<data dir>/languages` on the local disk, also when
//! the data directory is kept in a bucket (LEGEJO_STORAGE): they are not
//! durable data of the library, and can be imported again. Another server
//! process on the same data directory keeps the pack it has loaded until
//! it restarts.
//!
//! After an import or a removal the language's pack is loaded again; the
//! vocabulary worker counts the books in the language again in the
//! background, since each book remembers the id of the pack it was counted
//! with (vocabulary/index.rs).

use crate::auth::AdminUser;
use crate::langpack::Pack;
use crate::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path as FsPath;
use std::sync::Arc;

/// The largest zip taken.
pub const MAX_ZIP_BYTES: usize = 64 * 1024 * 1024;
/// The most a zip may unpack to, all files together.
const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;
/// The files a pack zip may hold, and the ones it must.
const ALLOWED: [&str; 4] = ["pack.json", "lemmas.tsv", "forms.tsv", "README.txt"];
const REQUIRED: [&str; 3] = ["pack.json", "lemmas.tsv", "forms.tsv"];
/// Where people find the packs, and how to make one: the page of the
/// default catalogue, linked whatever LEGEJO_LANGUAGES_URL says.
const CATALOGUE_PAGE: &str = crate::settings::LANGUAGES_URL;

/// One import or removal at a time in this process.
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// An error response, boxed so that the handlers' results stay small.
pub struct ApiError(Box<Response>);

impl From<Response> for ApiError {
    fn from(r: Response) -> ApiError {
        ApiError(Box::new(r))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        *self.0
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

#[derive(Serialize)]
pub struct Entry {
    code: String,
    name: String,
    version: String,
    source: String,
    license: String,
    attribution: String,
    lemmas: usize,
    forms: usize,
    /// Books on the instance in the language, of every user.
    books: i64,
}

fn entry(pack: &Pack, books: &HashMap<String, i64>) -> Entry {
    let m = &pack.meta;
    Entry {
        code: m.language.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        source: m.source.clone(),
        license: m.license.clone(),
        attribution: m.attribution.clone(),
        lemmas: pack.lemma_count(),
        forms: pack.form_count(),
        books: books.get(&m.language).copied().unwrap_or(0),
    }
}

async fn books_by_language(state: &AppState) -> Result<HashMap<String, i64>, ApiError> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT language, COUNT(*) FROM books WHERE language IS NOT NULL GROUP BY language")
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::from(internal(e.into())))?;
    Ok(rows.into_iter().collect())
}

/// A pack of the catalogue, as the admin page shows it.
#[derive(Serialize)]
pub struct Available {
    code: String,
    name: String,
    english: String,
    version: String,
    lemmas: u64,
    /// The size of the download.
    bytes: u64,
    license: String,
    attribution: String,
    /// The version installed, if the language has a pack.
    installed_version: Option<String>,
    /// The catalogue's version is newer than the one installed.
    update: bool,
    books: i64,
}

#[derive(Serialize)]
pub struct Esperanto {
    books: i64,
}

#[derive(Serialize)]
pub struct List {
    packs: Vec<Entry>,
    /// Built in, through ESPDIC: always there.
    esperanto: Esperanto,
    /// The page about packs on legejo.se.
    catalogue: &'static str,
    /// Where the catalogue is fetched from; None when it is off.
    catalogue_url: Option<String>,
    catalogue_error: Option<String>,
    available: Vec<Available>,
    /// Languages with books on the instance, a pack in the catalogue and
    /// none installed: most books first.
    suggested: Vec<String>,
}

/// GET /api/admin/languages
pub async fn list(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<List>, ApiError> {
    let books = books_by_language(&state).await?;
    let packs = state.packs.clone();
    let installed = tokio::task::spawn_blocking(move || packs.installed_packs()).await.map_err(|e| ApiError::from(internal(e.into())))?;
    let mut packs: Vec<Entry> = installed.iter().map(|p| entry(p, &books)).collect();
    packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.code.cmp(&b.code)));
    let (available, catalogue_error) = match crate::langcatalogue::catalogue(&state, false).await {
        None => (Vec::new(), None),
        Some(Err(e)) => (Vec::new(), Some(e)),
        Some(Ok(catalogue)) => {
            let offers = catalogue
                .packs
                .iter()
                .map(|o| {
                    let installed = packs.iter().find(|p| p.code == o.code).map(|p| p.version.clone());
                    Available {
                        code: o.code.clone(),
                        name: o.name.clone(),
                        english: o.english.clone(),
                        version: o.version.clone(),
                        lemmas: o.lemmas,
                        bytes: o.bytes,
                        license: o.license.clone(),
                        attribution: o.attribution.clone(),
                        update: installed.as_deref().is_some_and(|v| crate::langcatalogue::newer(&o.version, v)),
                        installed_version: installed,
                        books: books.get(&o.code).copied().unwrap_or(0),
                    }
                })
                .collect();
            (offers, None)
        }
    };
    let mut suggested: Vec<&Available> = available.iter().filter(|a| a.installed_version.is_none() && a.books > 0).collect();
    suggested.sort_by(|a, b| b.books.cmp(&a.books).then_with(|| a.code.cmp(&b.code)));
    let suggested = suggested.into_iter().map(|a| a.code.clone()).collect();
    Ok(Json(List {
        packs,
        esperanto: Esperanto { books: books.get("eo").copied().unwrap_or(0) },
        catalogue: CATALOGUE_PAGE,
        catalogue_url: state.settings.languages_url.clone(),
        catalogue_error,
        available,
        suggested,
    }))
}

/// Why an import failed: the zip (422, said to the admin) or the server.
pub(crate) enum Failure {
    Bad(String),
    Internal(anyhow::Error),
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Failure {
        Failure::Internal(e.into())
    }
}

fn bad(message: impl Into<String>) -> Failure {
    Failure::Bad(message.into())
}

/// The files of a pack zip, by name, read in memory: at the root or under
/// one top folder, only the allowed names, at most MAX_UNPACKED_BYTES in
/// all. The metadata some systems add when they zip a folder (`__MACOSX/`,
/// `.DS_Store`) is passed over.
fn unzip(bytes: &[u8]) -> Result<Vec<(&'static str, Vec<u8>)>, Failure> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(format!("not a zip file ({e})")))?;
    // First the names and declared sizes, before anything is unpacked.
    let mut top: Option<String> = None;
    let mut at_root = false;
    let mut wanted: Vec<(usize, &'static str)> = Vec::new();
    let mut declared = 0u64;
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i).map_err(|e| bad(format!("the zip cannot be read ({e})")))?;
        let name = file.name().map_err(|e| bad(format!("the zip cannot be read ({e})")))?.to_string();
        if name.starts_with("__MACOSX/") || name == "__MACOSX" || name.rsplit('/').next() == Some(".DS_Store") {
            continue;
        }
        if name.contains('\\') || name.starts_with('/') || name.split('/').any(|p| p == ".." || p == ".") || file.enclosed_name().is_none() {
            return Err(bad(format!("\"{name}\": a path outside the pack")));
        }
        if file.is_symlink() {
            return Err(bad(format!("\"{name}\": a link, not a file")));
        }
        let parts: Vec<&str> = name.trim_end_matches('/').split('/').collect();
        if file.is_dir() {
            if parts.len() != 1 {
                return Err(bad(format!("\"{name}\": folders inside the pack are not allowed")));
            }
            match &top {
                Some(t) if t != parts[0] => return Err(bad("the files must be at the root of the zip or in one folder")),
                _ => top = Some(parts[0].to_string()),
            }
            continue;
        }
        let base = match parts.as_slice() {
            [base] => {
                at_root = true;
                *base
            }
            [folder, base] => {
                match &top {
                    Some(t) if t != folder => return Err(bad("the files must be at the root of the zip or in one folder")),
                    _ => top = Some(folder.to_string()),
                }
                *base
            }
            _ => return Err(bad(format!("\"{name}\": folders inside the pack are not allowed"))),
        };
        if at_root && top.is_some() {
            return Err(bad("the files must be at the root of the zip or in one folder"));
        }
        let Some(allowed) = ALLOWED.iter().find(|a| **a == base) else {
            return Err(bad(format!("\"{name}\": a pack holds only {}", ALLOWED.join(", "))));
        };
        if wanted.iter().any(|(_, n)| n == allowed) {
            return Err(bad(format!("\"{base}\" is in the zip twice")));
        }
        declared = declared.saturating_add(file.size());
        if declared > MAX_UNPACKED_BYTES {
            return Err(bad(format!("the pack unpacks to more than {} MB", MAX_UNPACKED_BYTES / (1024 * 1024))));
        }
        wanted.push((i, allowed));
    }
    if at_root && top.is_some() {
        return Err(bad("the files must be at the root of the zip or in one folder"));
    }
    for name in REQUIRED {
        if !wanted.iter().any(|(_, n)| *n == name) {
            return Err(bad(format!("the zip has no {name}")));
        }
    }
    // Then the files, counted as they are read: a declared size can lie.
    let mut total = 0u64;
    let mut out = Vec::new();
    for (i, name) in wanted {
        let file = archive.by_index(i).map_err(|e| bad(format!("{name} cannot be unpacked ({e})")))?;
        let mut data = Vec::new();
        file.take(MAX_UNPACKED_BYTES - total + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("{name} cannot be unpacked ({e})")))?;
        total += data.len() as u64;
        if total > MAX_UNPACKED_BYTES {
            return Err(bad(format!("the pack unpacks to more than {} MB", MAX_UNPACKED_BYTES / (1024 * 1024))));
        }
        out.push((name, data));
    }
    Ok(out)
}

/// A hidden directory next to the packs ("." and not a language code, so
/// never taken for one).
fn hidden(languages: &FsPath, code: &str, what: &str) -> std::path::PathBuf {
    languages.join(format!(".{code}.{what}-{}", crate::books::new_uuid()))
}

/// Unpack, check and install a pack zip; the language's code, and whether
/// it was installed now. `expect` is the language the pack must be for.
/// With `if_missing`, a pack there already is kept, also when it appears
/// while this one is unpacked (another process installing it too).
fn install(languages: &FsPath, bytes: &[u8], expect: Option<&str>, if_missing: bool) -> Result<(String, bool), Failure> {
    let files = unzip(bytes)?;
    let json = &files.iter().find(|(n, _)| *n == "pack.json").expect("required").1;
    let meta: crate::langpack::Meta = serde_json::from_slice(json).map_err(|e| bad(format!("pack.json: {e}")))?;
    let code = meta.language;
    if code == "eo" {
        return Err(bad("Esperanto is built in and cannot be replaced by a pack"));
    }
    if !crate::language::is_code(&code) {
        return Err(bad(format!("pack.json: \"{code}\" is not a language code (two or three lower-case letters)")));
    }
    if let Some(expect) = expect.filter(|e| *e != code) {
        return Err(bad(format!("pack.json says the pack is for \"{code}\", not \"{expect}\"")));
    }
    let there = |dir: &FsPath| dir.join("pack.json").is_file();
    if if_missing && there(&languages.join(&code)) {
        return Ok((code, false));
    }
    std::fs::create_dir_all(languages)?;
    let tmp = hidden(languages, &code, "tmp");
    let result = (|| {
        std::fs::create_dir(&tmp)?;
        for (name, data) in &files {
            std::fs::write(tmp.join(name), data)?;
        }
        crate::langpack::check_dir(&tmp, &code).map_err(|e| bad(format!("the pack cannot be loaded: {e:#}")))?;
        let dir = languages.join(&code);
        if if_missing && there(&dir) {
            return Ok(false);
        }
        let old = if dir.exists() {
            let old = hidden(languages, &code, "old");
            std::fs::rename(&dir, &old)?;
            Some(old)
        } else {
            None
        };
        if let Err(e) = std::fs::rename(&tmp, &dir) {
            if let Some(old) = &old {
                let _ = std::fs::rename(old, &dir);
            }
            if if_missing && there(&dir) {
                return Ok(false);
            }
            return Err(e.into());
        }
        if let Some(old) = old {
            if let Err(e) = std::fs::remove_dir_all(&old) {
                tracing::warn!("language pack: could not remove {}: {e}", old.display());
            }
        }
        Ok(true)
    })();
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    result.map(|now| (code, now))
}

/// Install a pack zip and load it: the one way in for an imported zip and a
/// download from the catalogue alike. The pack, and whether it was
/// installed now (see `install` for `expect` and `if_missing`).
pub(crate) async fn install_zip(state: &AppState, bytes: Vec<u8>, expect: Option<&str>, if_missing: bool) -> Result<(Arc<Pack>, bool), Failure> {
    let _busy = BUSY.lock().await;
    let languages = state.packs.dir();
    let packs = state.packs.clone();
    let expect = expect.map(str::to_string);
    tokio::task::spawn_blocking(move || -> Result<(Arc<Pack>, bool), Failure> {
        let (code, now) = install(&languages, &bytes, expect.as_deref(), if_missing)?;
        let pack = packs.reload(&code).ok_or_else(|| Failure::Internal(anyhow::anyhow!("the pack for {code} was installed but does not load")))?;
        Ok((pack, now))
    })
    .await
    .map_err(|e| Failure::Internal(e.into()))?
}

/// POST /api/admin/languages: a pack zip in the multipart field `file`.
pub async fn import(State(state): State<AppState>, admin: AdminUser, mut multipart: Multipart) -> Result<Json<Entry>, ApiError> {
    let mut bytes = None;
    while let Some(field) = multipart.next_field().await.map_err(crate::images::multipart_error)? {
        if field.name() != Some("file") {
            continue;
        }
        bytes = Some(field.bytes().await.map_err(crate::images::multipart_error)?);
        break;
    }
    let Some(bytes) = bytes.filter(|b| !b.is_empty()) else {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "no file").into());
    };
    if bytes.len() > MAX_ZIP_BYTES {
        return Err(error(StatusCode::PAYLOAD_TOO_LARGE, format!("the zip is larger than {} MB", MAX_ZIP_BYTES / (1024 * 1024))).into());
    }
    let pack = match install_zip(&state, Vec::from(bytes), None, false).await {
        Ok((pack, _)) => pack,
        Err(Failure::Bad(message)) => return Err(error(StatusCode::UNPROCESSABLE_ENTITY, message).into()),
        Err(Failure::Internal(e)) => return Err(internal(e).into()),
    };
    tracing::info!("admin: {} imported the language pack {} version {}", admin.0.username, pack.meta.language, pack.meta.version);
    crate::audit::log(
        &state,
        crate::audit::by(&admin.0),
        "admin.language_imported",
        serde_json::json!({ "code": pack.meta.language, "name": pack.meta.name, "version": pack.meta.version }),
    )
    .await;
    let books = books_by_language(&state).await?;
    Ok(Json(entry(&pack, &books)))
}

#[derive(Deserialize)]
pub struct InstallForm {
    code: String,
}

/// POST /api/admin/languages/install `{"code": "nl"}`: the catalogue's pack
/// for a language, downloaded and installed (or updated).
pub async fn install_from_catalogue(State(state): State<AppState>, admin: AdminUser, Json(form): Json<InstallForm>) -> Result<Json<Entry>, ApiError> {
    use crate::langcatalogue::InstallError;
    let code = form.code;
    if code == "eo" || !crate::language::is_code(&code) {
        return Err(error(StatusCode::NOT_FOUND, "the catalogue has no pack for this language").into());
    }
    let pack = match crate::langcatalogue::install(&state, &code, false).await {
        Ok((pack, _)) => pack,
        Err(InstallError::Off) => return Err(error(StatusCode::CONFLICT, "the catalogue is off (LEGEJO_LANGUAGES_URL)").into()),
        Err(InstallError::Unknown) => return Err(error(StatusCode::NOT_FOUND, "the catalogue has no pack for this language").into()),
        Err(InstallError::Upstream(message)) => return Err(error(StatusCode::BAD_GATEWAY, message).into()),
        Err(InstallError::Internal(e)) => return Err(internal(e).into()),
    };
    tracing::info!("admin: {} installed the language pack {} version {} from the catalogue", admin.0.username, pack.meta.language, pack.meta.version);
    crate::audit::log(
        &state,
        crate::audit::by(&admin.0),
        "admin.language_installed",
        serde_json::json!({ "code": pack.meta.language, "name": pack.meta.name, "version": pack.meta.version }),
    )
    .await;
    let books = books_by_language(&state).await?;
    Ok(Json(entry(&pack, &books)))
}

/// DELETE /api/admin/languages/{code}: an installed pack; the books in the
/// language have no study list until a pack comes again.
pub async fn remove(State(state): State<AppState>, admin: AdminUser, Path(code): Path<String>) -> Result<StatusCode, ApiError> {
    if !crate::language::is_code(&code) {
        return Err(error(StatusCode::NOT_FOUND, "not found").into());
    }
    let _busy = BUSY.lock().await;
    let languages = state.packs.dir();
    let dir = languages.join(&code);
    if !dir.is_dir() {
        return Err(error(StatusCode::NOT_FOUND, "not found").into());
    }
    let name = state.packs.get(&code).map(|p| p.meta.name.clone()).unwrap_or_default();
    let packs = state.packs.clone();
    let code_ = code.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        // Out of the way first, in one rename; then gone.
        let old = hidden(&languages, &code_, "old");
        std::fs::rename(&dir, &old)?;
        if let Err(e) = std::fs::remove_dir_all(&old) {
            tracing::warn!("language pack: could not remove {}: {e}", old.display());
        }
        packs.reload(&code_);
        Ok(())
    })
    .await
    .map_err(|e| ApiError::from(internal(e.into())))?
    .map_err(|e| ApiError::from(internal(e.into())))?;
    tracing::info!("admin: {} removed the language pack {code}", admin.0.username);
    crate::audit::log(&state, crate::audit::by(&admin.0), "admin.language_removed", serde_json::json!({ "code": code, "name": name })).await;
    Ok(StatusCode::NO_CONTENT)
}
