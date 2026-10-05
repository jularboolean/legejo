//! Watched import folder: EPUB files dropped into LEGEJO_IMPORT_DIR become
//! books of LEGEJO_IMPORT_USER (default: the oldest admin).
//!
//! Every LEGEJO_IMPORT_INTERVAL seconds the folder (and its subfolders) is
//! scanned. A file is left alone until it has not changed for a while, so
//! one still being copied is not imported half-written. It is then claimed
//! by renaming it into `.legejo/`: a rename succeeds for exactly one
//! process, so two server processes never import the same file.
//! Afterwards it moves to `imported/` (or is deleted with
//! LEGEJO_IMPORT_DELETE), to `duplicates/` when the owner already has the
//! same file, or to `failed/` when it is not a readable EPUB.

use crate::settings::ImportDir;
use crate::AppState;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const WORK: &str = ".legejo";
const IMPORTED: &str = "imported";
const DUPLICATES: &str = "duplicates";
const FAILED: &str = "failed";
/// A file must have been left alone this long before it is imported.
const SETTLE: Duration = Duration::from_secs(30);
/// A claimed file still in .legejo after this long belongs to a process
/// that died; it goes back to the folder.
const STALE: Duration = Duration::from_secs(15 * 60);

pub async fn worker(state: AppState) {
    let Some(config) = state.settings.import.clone() else {
        return;
    };
    tracing::info!("import folder: watching {} every {} s", config.dir.display(), config.interval_secs);
    let mut warned = false;
    loop {
        match scan(&state, &config, SETTLE).await {
            Ok(_) => warned = false,
            Err(e) if !warned => {
                tracing::warn!("import folder {}: {e:#}", config.dir.display());
                warned = true;
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(config.interval_secs)).await;
    }
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || [IMPORTED, DUPLICATES, FAILED].contains(&name)
}

fn is_epub(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("epub"))
        && !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'))
}

/// EPUB files waiting in the folder (subfolders included).
fn candidates(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                let name = entry.file_name();
                if !skip_dir(&name.to_string_lossy()) {
                    dirs.push(path);
                }
            } else if kind.is_file() && is_epub(&path) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

pub fn pending(root: &Path) -> usize {
    candidates(root).len()
}

/// `name` in `dir`, or `name (2)` … when taken.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn move_to(root: &Path, from: &Path, sub: &str, name: &str) -> std::io::Result<()> {
    let dir = root.join(sub);
    std::fs::create_dir_all(&dir)?;
    std::fs::rename(from, free_name(&dir, name))
}

/// The claimed file's name without its "<id>-" prefix.
fn original_name(claimed: &Path) -> String {
    let name = claimed.file_name().and_then(|n| n.to_str()).unwrap_or("book.epub");
    name.split_once('-').map(|(_, rest)| rest).unwrap_or(name).to_string()
}

async fn owner(state: &AppState, config: &ImportDir) -> anyhow::Result<(i64, String)> {
    let row: Option<(i64, String)> = match &config.user {
        Some(name) => sqlx::query_as("SELECT id, username FROM users WHERE LOWER(username) = LOWER($1)")
            .bind(name.trim())
            .fetch_optional(&state.db)
            .await?,
        None => sqlx::query_as("SELECT id, username FROM users WHERE is_admin <> 0 ORDER BY id LIMIT 1")
            .fetch_optional(&state.db)
            .await?,
    };
    row.ok_or_else(|| match &config.user {
        Some(name) => anyhow::anyhow!("LEGEJO_IMPORT_USER={name}: no such user"),
        None => anyhow::anyhow!("no admin account to import to"),
    })
}

/// One pass over the folder; returns how many books were added.
pub(crate) async fn scan(state: &AppState, config: &ImportDir, settle: Duration) -> anyhow::Result<usize> {
    let root = config.dir.clone();
    if !root.is_dir() {
        anyhow::bail!("not a directory");
    }
    let work = root.join(WORK);
    std::fs::create_dir_all(&work)?;
    recover_stale(&root, &work);

    let files: Vec<PathBuf> = candidates(&root)
        .into_iter()
        .filter(|p| {
            std::fs::metadata(p).is_ok_and(|m| {
                m.len() > 0 && m.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= settle)
            })
        })
        .collect();
    if files.is_empty() {
        return Ok(0);
    }
    let (owner_id, owner_name) = owner(state, config).await?;

    let mut added = 0;
    for file in files {
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("book.epub").to_string();
        let claimed = work.join(format!("{}-{name}", crate::books::new_uuid()));
        if std::fs::rename(&file, &claimed).is_err() {
            continue; // The other process took it, or it vanished.
        }
        // Stamp the claim: a rename keeps the old mtime, and the stale check
        // in the other process must not take the file back mid-import.
        let _ = std::fs::File::options().write(true).open(&claimed).and_then(|f| f.set_modified(SystemTime::now()));
        match import_one(state, owner_id, &owner_name, &claimed, &name).await {
            Outcome::Added => {
                added += 1;
                let done = if config.delete { std::fs::remove_file(&claimed) } else { move_to(&root, &claimed, IMPORTED, &name) };
                if let Err(e) = done {
                    tracing::warn!("import folder: {name} was imported but could not be moved away: {e}");
                }
            }
            Outcome::Duplicate => {
                tracing::info!("import folder: {name} is already in {owner_name}'s library");
                let _ = move_to(&root, &claimed, DUPLICATES, &name);
            }
            Outcome::Failed(why) => {
                tracing::warn!("import folder: {name}: {why}");
                crate::audit::log(state, None, "book.import_failed", json!({ "file": name, "error": why })).await;
                let _ = move_to(&root, &claimed, FAILED, &name);
            }
        }
    }
    Ok(added)
}

fn recover_stale(root: &Path, work: &Path) {
    let Ok(entries) = std::fs::read_dir(work) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let stale = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age >= STALE);
        if stale {
            let _ = std::fs::rename(&path, free_name(root, &original_name(&path)));
        }
    }
}

enum Outcome {
    Added,
    Duplicate,
    Failed(String),
}

async fn import_one(state: &AppState, owner_id: i64, owner_name: &str, path: &Path, name: &str) -> Outcome {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) => return Outcome::Failed(format!("cannot read the file: {e}")),
    };
    if bytes.len() > state.settings.max_upload_bytes {
        return Outcome::Failed(format!("larger than LEGEJO_MAX_UPLOAD_MB ({} MB)", state.settings.max_upload_bytes / (1024 * 1024)));
    }
    let sha = crate::books::sha256_hex(&bytes);
    let same: Result<Option<i64>, _> = sqlx::query_scalar("SELECT id FROM books WHERE owner_id = $1 AND file_sha256 = $2 LIMIT 1")
        .bind(owner_id)
        .bind(&sha)
        .fetch_optional(&state.db)
        .await;
    match same {
        Ok(Some(_)) => return Outcome::Duplicate,
        Ok(None) => {}
        Err(e) => return Outcome::Failed(format!("database: {e}")),
    }
    match crate::books::store_epub(state, owner_id, &bytes, name).await {
        Ok(Ok(book)) => {
            crate::audit::log(
                state,
                None,
                "book.folder_imported",
                json!({ "book_id": book.id, "title": book.title, "owner": owner_name, "file": name }),
            )
            .await;
            Outcome::Added
        }
        Ok(Err(e)) => Outcome::Failed(e),
        Err(_) => Outcome::Failed("internal error while storing the book".into()),
    }
}
