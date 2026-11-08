//! The catalogue of language packs: `<base>/index.json` lists the packs
//! that can be downloaded, each as `<base>/<code>.zip`, with its version
//! and the sha256 of the zip. The base is LEGEJO_LANGUAGES_URL (settings.rs),
//! by default https://legejo.se/languages; set to the empty string, there is
//! no catalogue and packs come only from a file (langadmin.rs).
//!
//! The index is fetched when an admin looks at the packs, and kept in memory
//! for an hour (a failure for a minute). A zip is downloaded whole into
//! memory, at most `langadmin::MAX_ZIP_BYTES`, checked against the sha256
//! the index gives, and installed as an imported zip is.
//!
//! Instances that had Spanish, French or German built in (before packs were
//! downloaded) get those packs from the catalogue once: `upgrade_built_in`,
//! run by the vocabulary worker. The settings table remembers which
//! languages are still to come (`language_packs_pending`) and that it is
//! done (`language_packs_migrated`), so that a pack an admin removes later
//! stays removed.

use crate::langadmin::{Failure, MAX_ZIP_BYTES};
use crate::langpack::Pack;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// What the catalogue is asked with; legejo.se counts downloads by it.
const USER_AGENT: &str = concat!("Legejo/", env!("CARGO_PKG_VERSION"), " (+https://legejo.se)");
const KEEP: Duration = Duration::from_secs(3600);
const KEEP_FAILURE: Duration = Duration::from_secs(60);
const INDEX_TIMEOUT: Duration = Duration::from_secs(10);
const INDEX_MAX_BYTES: usize = 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// One pack of the catalogue.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Offer {
    pub code: String,
    /// The language's own name for itself ("Nederlands").
    pub name: String,
    /// Its name in English ("Dutch").
    #[serde(default)]
    pub english: String,
    pub version: String,
    #[serde(default)]
    pub lemmas: u64,
    #[serde(default)]
    pub forms: u64,
    /// The size of the zip.
    #[serde(default)]
    pub bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub attribution: String,
}

#[derive(Deserialize)]
struct Index {
    format: u32,
    #[serde(default)]
    packs: Vec<Offer>,
}

pub struct Catalogue {
    pub packs: Vec<Offer>,
}

impl Catalogue {
    pub fn find(&self, code: &str) -> Option<&Offer> {
        self.packs.iter().find(|p| p.code == code)
    }
}

type Cached = (Instant, Result<Arc<Catalogue>, String>);

/// The catalogues fetched, by base URL.
static CACHE: LazyLock<Mutex<HashMap<String, Cached>>> = LazyLock::new(Default::default);

/// The catalogue, or why it cannot be had; None when there is none
/// (LEGEJO_LANGUAGES_URL empty). `fresh` fetches it again whatever is kept.
pub async fn catalogue(state: &AppState, fresh: bool) -> Option<Result<Arc<Catalogue>, String>> {
    let base = state.settings.languages_url.clone()?;
    if !fresh {
        if let Some((at, kept)) = CACHE.lock().unwrap().get(&base) {
            let keep = if kept.is_ok() { KEEP } else { KEEP_FAILURE };
            if at.elapsed() < keep {
                return Some(kept.clone());
            }
        }
    }
    let fetched = fetch_index(&state.http, &base).await.map(Arc::new);
    if let Err(e) = &fetched {
        tracing::warn!("language catalogue {base}: {e}");
    }
    CACHE.lock().unwrap().insert(base, (Instant::now(), fetched.clone()));
    Some(fetched)
}

async fn fetch_index(http: &reqwest::Client, base: &str) -> Result<Catalogue, String> {
    let bytes = get_capped(http, &format!("{base}/index.json"), INDEX_MAX_BYTES, INDEX_TIMEOUT).await?;
    let index: Index = serde_json::from_slice(&bytes).map_err(|e| format!("the catalogue cannot be read ({e})"))?;
    if index.format != 1 {
        return Err(format!("the catalogue's format {} is not known", index.format));
    }
    let packs = index.packs.into_iter().filter(|p| p.code != "eo" && crate::language::is_code(&p.code)).collect();
    Ok(Catalogue { packs })
}

/// A body of at most `cap` bytes, read as it comes so that a larger one is
/// cut off rather than held.
async fn get_capped(http: &reqwest::Client, url: &str, cap: usize, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut res = http
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("{url} cannot be reached ({})", without_url(&e)))?;
    if !res.status().is_success() {
        return Err(format!("{url} answers HTTP {}", res.status().as_u16()));
    }
    let too_large = || format!("{url} is larger than {} MB", cap / (1024 * 1024));
    if res.content_length().is_some_and(|n| n > cap as u64) {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| format!("{url} could not be read to the end ({})", without_url(&e)))? {
        if body.len() + chunk.len() > cap {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// A reqwest error without the URL, which the message names already.
fn without_url(e: &reqwest::Error) -> String {
    let mut e = e.to_string();
    if let Some(at) = e.find(" for url (") {
        e.truncate(at);
    }
    e
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether version `a` is newer than `b`: the runs of digits compared as
/// numbers ("2026-11-08" after "2026-9-30", "10" after "9"), the rest as text.
pub fn newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        let mut rest = v;
        while !rest.is_empty() {
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let (num, after) = rest.split_at(digits);
            let text = after.find(|c: char| c.is_ascii_digit()).unwrap_or(after.len());
            let (text, after) = after.split_at(text);
            out.push((num.parse().unwrap_or(0), text.to_string()));
            rest = after;
        }
        out
    }
    parts(a) > parts(b)
}

/// Why a pack could not be installed from the catalogue.
pub enum InstallError {
    /// There is no catalogue (LEGEJO_LANGUAGES_URL empty).
    Off,
    /// The catalogue has no pack for the language.
    Unknown,
    /// The catalogue or the download failed, or gave what does not fit.
    Upstream(String),
    Internal(anyhow::Error),
}

/// Download the catalogue's pack for a language and install it. With
/// `if_missing`, a pack that is there already (another process put it there
/// first) is left as it is: the pack, and false for "not installed now".
pub async fn install(state: &AppState, code: &str, if_missing: bool) -> Result<(Arc<Pack>, bool), InstallError> {
    let Some(base) = state.settings.languages_url.clone() else {
        return Err(InstallError::Off);
    };
    let found = catalogue(state, false).await.ok_or(InstallError::Off)?.map_err(InstallError::Upstream)?;
    let mut offer = found.find(code).cloned().ok_or(InstallError::Unknown)?;
    let url = format!("{base}/{code}.zip");
    let mut zip = get_capped(&state.http, &url, MAX_ZIP_BYTES, DOWNLOAD_TIMEOUT).await.map_err(InstallError::Upstream)?;
    let mut sum = sha256_hex(&zip);
    if !sum.eq_ignore_ascii_case(&offer.sha256) {
        // The catalogue kept here may be older than the zip: asked again,
        // a newer one may know it.
        let again = catalogue(state, true).await.ok_or(InstallError::Off)?.map_err(InstallError::Upstream)?;
        offer = again.find(code).cloned().ok_or(InstallError::Unknown)?;
        if !sum.eq_ignore_ascii_case(&offer.sha256) {
            zip = get_capped(&state.http, &url, MAX_ZIP_BYTES, DOWNLOAD_TIMEOUT).await.map_err(InstallError::Upstream)?;
            sum = sha256_hex(&zip);
        }
        if !sum.eq_ignore_ascii_case(&offer.sha256) {
            return Err(InstallError::Upstream(format!("{url} does not match the catalogue (sha256 {sum}, the catalogue says {})", offer.sha256)));
        }
    }
    match crate::langadmin::install_zip(state, zip, Some(code), if_missing).await {
        Ok(done) => Ok(done),
        Err(Failure::Bad(message)) => Err(InstallError::Upstream(format!("the pack from {url} cannot be installed: {message}"))),
        Err(Failure::Internal(e)) => Err(InstallError::Internal(e)),
    }
}

// ------------------------------------------------- the built-in packs' heirs

/// The languages that were built in until packs came from the catalogue.
const FORMERLY_BUILT_IN: [&str; 3] = ["es", "fr", "de"];
const MIGRATED: &str = "language_packs_migrated";
const PENDING: &str = "language_packs_pending";
const RETRY_FIRST: Duration = Duration::from_secs(3600);
const RETRY_MOST: Duration = Duration::from_secs(24 * 3600);

/// Where `upgrade_built_in` stands in this process.
pub struct Upgrade {
    done: bool,
    next: Option<Instant>,
    delay: Duration,
}

impl Default for Upgrade {
    fn default() -> Self {
        Upgrade { done: false, next: None, delay: RETRY_FIRST }
    }
}

/// Give an instance that used the built-in Spanish, French or German pack
/// those packs from the catalogue, once. A language is due when the
/// instance has books in it and a book counted with a study list (only a
/// pack gives one) and no pack is installed. Failing, it is tried again an
/// hour later, then less often. Two processes may both try: each install is
/// a rename, and one finding the pack there already leaves it.
pub async fn upgrade_built_in(state: &AppState, upgrade: &mut Upgrade) {
    if upgrade.done || upgrade.next.is_some_and(|at| Instant::now() < at) {
        return;
    }
    if state.settings.languages_url.is_none() {
        upgrade.done = true;
        return;
    }
    match upgrade_once(state).await {
        Ok(()) => upgrade.done = true,
        Err(e) => {
            tracing::warn!("language packs: {e:#}; tried again in {} min", upgrade.delay.as_secs() / 60);
            upgrade.next = Some(Instant::now() + upgrade.delay);
            upgrade.delay = (upgrade.delay * 2).min(RETRY_MOST);
        }
    }
}

async fn setting(state: &AppState, key: &str) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT value FROM settings WHERE key = $1").bind(key).fetch_optional(&state.db).await?)
}

async fn set_setting(state: &AppState, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// The languages formerly built in that the instance used: books in the
/// language, and a book of it counted with a study list.
async fn used_formerly_built_in(state: &AppState) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for code in FORMERLY_BUILT_IN {
        let used: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM vocabulary v WHERE v.language = $1 AND v.study IS NOT NULL
               AND EXISTS (SELECT 1 FROM books b WHERE b.language = $1) LIMIT 1",
        )
        .bind(code)
        .fetch_optional(&state.db)
        .await?;
        if used.is_some() {
            out.push(code.to_string());
        }
    }
    Ok(out)
}

pub(crate) async fn upgrade_once(state: &AppState) -> anyhow::Result<()> {
    if setting(state, MIGRATED).await?.as_deref() == Some("true") {
        return Ok(());
    }
    // Which languages are due is decided once and kept: once the books are
    // counted again without a pack, they no longer show it.
    let pending: Vec<String> = match setting(state, PENDING).await? {
        Some(list) => list.split(',').filter(|c| crate::language::is_code(c)).map(str::to_string).collect(),
        None => {
            let due = used_formerly_built_in(state).await?;
            set_setting(state, PENDING, &due.join(",")).await?;
            due
        }
    };
    let mut left = Vec::new();
    let mut failures = Vec::new();
    for code in pending {
        if state.packs.dir().join(&code).join("pack.json").is_file() {
            // Installed already, by an admin or another process.
            state.packs.reload(&code);
            continue;
        }
        match install(state, &code, true).await {
            Ok((pack, true)) => {
                tracing::info!("language packs: installed {code} version {} from the catalogue, in place of the built-in pack", pack.meta.version);
                crate::audit::log(
                    state,
                    None,
                    "admin.language_installed",
                    serde_json::json!({ "code": code, "name": pack.meta.name, "version": pack.meta.version, "automatic": true }),
                )
                .await;
            }
            Ok((_, false)) => {}
            Err(InstallError::Unknown) => tracing::warn!("language packs: the catalogue has no pack for {code}"),
            Err(InstallError::Off) => return Ok(()),
            Err(InstallError::Upstream(e)) => {
                failures.push(format!("{code}: {e}"));
                left.push(code);
            }
            Err(InstallError::Internal(e)) => {
                failures.push(format!("{code}: {e:#}"));
                left.push(code);
            }
        }
    }
    if left.is_empty() {
        set_setting(state, MIGRATED, "true").await?;
        return Ok(());
    }
    set_setting(state, PENDING, &left.join(",")).await?;
    anyhow::bail!("could not install {}", failures.join("; "))
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(newer("2026-11-09", "2026-11-08"));
        assert!(newer("2026-11-08", "2026-9-30"));
        assert!(newer("10", "9"));
        assert!(newer("1.2b", "1.2a"));
        assert!(!newer("2026-11-08", "2026-11-08"));
        assert!(!newer("1", "2026-11-08"));
    }
}
