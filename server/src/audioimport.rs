//! Watched import folder for audiobooks: what is dropped into
//! LEGEJO_AUDIOBOOK_IMPORT_DIR becomes audiobooks of
//! LEGEJO_AUDIOBOOK_IMPORT_USER (default: the oldest admin).
//!
//! Each entry at the top of the folder is one audiobook: a folder with its
//! audio files (mp3, m4a, m4b; subfolders such as "CD 1" included, heard in
//! the order of their names), or a single audio file. An image beside the
//! files (cover.jpg, folder.jpg, …) becomes the cover.
//!
//! The folder is scanned as the one for books is (importdir.rs): an entry is
//! left alone until nothing in it has changed for a while, claimed by a
//! rename into `.legejo/`, and afterwards moved to `imported/`, `duplicates/`
//! or `failed/`, or deleted with LEGEJO_AUDIOBOOK_IMPORT_DELETE. The audio
//! is never read into memory, and on the same disk as the data it is linked
//! rather than copied, so the size of an audiobook does not matter.

use crate::audiobooks::{audio_type, FolderBook, FolderOutcome};
use crate::importdir::{move_to, skip_dir, DUPLICATES, FAILED, IMPORTED, WORK};
use crate::settings::ImportDir;
use crate::AppState;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Nothing in an entry may have changed for this long before it is imported.
const SETTLE: Duration = Duration::from_secs(60);
/// A claimed entry still in .legejo after this long belongs to a process
/// that died; it goes back to the folder.
const STALE: Duration = Duration::from_secs(60 * 60);
const COVER_NAMES: [&str; 3] = ["cover", "folder", "front"];

pub async fn worker(state: AppState) {
    let Some(config) = state.settings.audio_import.clone() else {
        return;
    };
    tracing::info!("audiobook import folder: watching {} every {} s", config.dir.display(), config.interval_secs);
    let mut warned = false;
    loop {
        match scan(&state, &config, SETTLE).await {
            Ok(_) => warned = false,
            Err(e) if !warned => {
                tracing::warn!("audiobook import folder {}: {e:#}", config.dir.display());
                warned = true;
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(config.interval_secs)).await;
    }
}

fn hidden(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_none_or(|n| n.starts_with('.'))
}

fn is_audio(path: &Path) -> bool {
    !hidden(path) && path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains('.') && audio_type(n).is_some())
}

/// Every file below `dir`, hidden ones left out.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if hidden(&path) {
                continue;
            }
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() {
                found.push(path);
            }
        }
    }
    found
}

/// What an entry holds, read as an audiobook; None when it has no audio.
fn read_entry(entry: &Path, name: &str) -> Option<FolderBook> {
    if entry.is_file() {
        return is_audio(entry).then(|| FolderBook { name: name.to_string(), files: vec![entry.to_path_buf()], cover: None, single: true });
    }
    let all = files_below(entry);
    let mut files: Vec<PathBuf> = all.iter().filter(|p| is_audio(p)).cloned().collect();
    if files.is_empty() {
        return None;
    }
    // By folder, then by name, numbers by their value: "CD 2/1" before "CD 10/1".
    files.sort_by_cached_key(|p| {
        p.strip_prefix(entry).unwrap_or(p).components().map(|c| crate::formats::natural_key(&c.as_os_str().to_string_lossy())).collect::<Vec<_>>()
    });
    let image = |p: &&PathBuf| p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["jpg", "jpeg", "png"].iter().any(|x| e.eq_ignore_ascii_case(x)));
    let named = |p: &&PathBuf| p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| COVER_NAMES.iter().any(|n| s.eq_ignore_ascii_case(n)));
    let mut images: Vec<&PathBuf> = all.iter().filter(image).collect();
    images.sort();
    let cover = images.iter().find(|p| named(p)).or(images.first()).map(|p| p.to_path_buf());
    Some(FolderBook { name: name.to_string(), files, cover, single: false })
}

/// The entries at the top of the folder that hold audio.
fn candidates(root: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut found: Vec<(PathBuf, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let kind = entry.file_type().ok()?;
            let wanted = if kind.is_dir() { !skip_dir(&name) && files_below(&path).iter().any(|p| is_audio(p)) } else { kind.is_file() && is_audio(&path) };
            wanted.then_some((path, name))
        })
        .collect();
    found.sort();
    found
}

pub fn pending(root: &Path) -> usize {
    candidates(root).len()
}

/// When the entry, or anything in it, was last changed.
fn last_change(entry: &Path) -> Option<SystemTime> {
    let own = std::fs::metadata(entry).ok()?.modified().ok()?;
    if entry.is_file() {
        return Some(own);
    }
    files_below(entry).iter().filter_map(|p| std::fs::metadata(p).ok()?.modified().ok()).chain([own]).max()
}

fn touch(path: &Path) {
    let now = SystemTime::now();
    let _ = if path.is_dir() { std::fs::File::open(path).and_then(|f| f.set_modified(now)) } else { std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(now)) };
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
        Some(name) => anyhow::anyhow!("LEGEJO_AUDIOBOOK_IMPORT_USER={name}: no such user"),
        None => anyhow::anyhow!("no admin account to import to"),
    })
}

/// One pass over the folder; returns how many audiobooks were added.
pub(crate) async fn scan(state: &AppState, config: &ImportDir, settle: Duration) -> anyhow::Result<usize> {
    let root = config.dir.clone();
    if !root.is_dir() {
        anyhow::bail!("not a directory");
    }
    let work = root.join(WORK);
    std::fs::create_dir_all(&work)?;
    recover_stale(&root, &work);

    let settled: Vec<(PathBuf, String)> = candidates(&root)
        .into_iter()
        .filter(|(path, _)| last_change(path).and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= settle))
        .collect();
    if settled.is_empty() {
        return Ok(0);
    }
    if !crate::audiobooks::enabled(state).await {
        anyhow::bail!("audiobooks are turned off; an admin turns them on at the admin page");
    }
    let (owner_id, owner_name) = owner(state, config).await?;

    let mut added = 0;
    for (entry, name) in settled {
        let claimed = work.join(format!("{}-{name}", crate::books::new_uuid()));
        if std::fs::rename(&entry, &claimed).is_err() {
            continue; // The other process took it, or it vanished.
        }
        // Stamp the claim: a rename keeps the old mtime, and the stale check
        // in the other process must not take the entry back mid-import.
        touch(&claimed);
        let outcome = match read_entry(&claimed, &name) {
            Some(found) => crate::audiobooks::import_folder(state, owner_id, &found).await,
            None => Err(anyhow::anyhow!("no audio files")),
        };
        match outcome {
            Ok(FolderOutcome::Added { id, title }) => {
                added += 1;
                crate::audit::log(
                    state,
                    None,
                    "audiobook.folder_imported",
                    json!({ "audiobook_id": id, "title": title, "owner": owner_name, "file": name }),
                )
                .await;
                let done = if !config.delete {
                    move_to(&root, &claimed, IMPORTED, &name)
                } else if claimed.is_dir() {
                    std::fs::remove_dir_all(&claimed)
                } else {
                    std::fs::remove_file(&claimed)
                };
                if let Err(e) = done {
                    tracing::warn!("audiobook import folder: {name} was imported but could not be moved away: {e}");
                }
            }
            Ok(FolderOutcome::Duplicate) => {
                tracing::info!("audiobook import folder: {name} is already among {owner_name}'s audiobooks");
                let _ = move_to(&root, &claimed, DUPLICATES, &name);
            }
            Err(e) => {
                let why = format!("{e:#}");
                tracing::warn!("audiobook import folder: {name}: {why}");
                crate::audit::log(state, None, "audiobook.import_failed", json!({ "file": name, "error": why })).await;
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
        let name = entry.file_name().to_string_lossy().into_owned();
        // Only what this importer claimed: the folder may be shared with
        // the one for books.
        if !(path.is_dir() || is_audio(&path)) {
            continue;
        }
        let stale = last_change(&path).and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= STALE);
        if stale {
            let original = name.split_once('-').map(|(_, rest)| rest).unwrap_or(&name);
            let _ = std::fs::rename(&path, crate::importdir::free_name(root, original));
        }
    }
}
