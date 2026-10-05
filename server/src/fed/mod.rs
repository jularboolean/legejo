//! Federated shelves over ActivityPub.
//!
//! A shelf with visibility 'federated' becomes an ActivityPub actor
//! (`Service`) that others can follow; its books go out as `Page` objects
//! and may only be free books (license.rs is the gate, checked on both the
//! sending and the receiving side). Users follow remote shelves through the
//! instance actor (`Application`), one Follow per remote shelf.
//!
//! Nothing here is reachable unless LEGEJO_PUBLIC_URL is set AND the admin
//! setting federation_mode is not "off" (the default): every endpoint then
//! answers 404.

pub mod admin;
pub mod deliver;
pub mod inbox;
pub mod net;
pub mod objects;
pub mod pages;
pub mod reconcile;
pub mod remote;
pub mod requests;
pub mod routes;
pub mod sig;

use crate::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const AS_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";
pub const AP_JSON: &str = "application/activity+json";

/// Where this instance lives on the web, from LEGEJO_PUBLIC_URL.
#[derive(Clone, Debug)]
pub struct FedConfig {
    /// "https://a.example", no trailing slash.
    pub base: String,
    /// "a.example" (with :port when one is given).
    pub host: String,
    /// LEGEJO_FED_ALLOW_PRIVATE=1: allow http and private addresses, for
    /// two instances on one machine in development. Never in production:
    /// the SSRF filter (net.rs) is what keeps internal services unreachable.
    pub allow_private: bool,
}

impl FedConfig {
    pub fn from_env() -> Option<FedConfig> {
        let raw = crate::settings::var_lossy("LEGEJO_PUBLIC_URL")?;
        let base = raw.trim().trim_end_matches('/').to_string();
        let allow_private = crate::settings::var_lossy("LEGEJO_FED_ALLOW_PRIVATE").is_some_and(|v| v == "1");
        let url = reqwest::Url::parse(&base).ok()?;
        if url.scheme() != "https" && !allow_private {
            tracing::warn!("LEGEJO_PUBLIC_URL must be https; federation unavailable");
            return None;
        }
        let host = match url.port() {
            Some(p) => format!("{}:{p}", url.host_str()?),
            None => url.host_str()?.to_string(),
        };
        Some(FedConfig { base, host, allow_private })
    }

    pub fn instance_actor(&self) -> String {
        format!("{}/ap/actor", self.base)
    }
    pub fn shelf_actor(&self, slug: &str) -> String {
        format!("{}/ap/shelves/{slug}", self.base)
    }
    pub fn book_iri(&self, uuid: &str) -> String {
        format!("{}/ap/books/{uuid}", self.base)
    }
    pub fn shared_inbox(&self) -> String {
        format!("{}/ap/inbox", self.base)
    }
    /// The slug, when `iri` is one of our shelf actors.
    pub fn slug_of(&self, iri: &str) -> Option<String> {
        let rest = iri.strip_prefix(&format!("{}/ap/shelves/", self.base))?;
        (!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
    }
}

/// Federation runtime shared through AppState.
pub struct Fed {
    pub config: Option<FedConfig>,
    /// SSRF-filtered client for everything fetched from other servers.
    pub http: reqwest::Client,
    /// Pokes the delivery worker and reconciler after a change.
    pub wake: tokio::sync::Notify,
    /// Incoming activities per domain in the current minute (rate limit).
    pub rate: Mutex<HashMap<String, (std::time::Instant, u32)>>,
}

impl Fed {
    pub fn new(config: Option<FedConfig>) -> anyhow::Result<Arc<Fed>> {
        let allow_private = config.as_ref().is_some_and(|c| c.allow_private);
        let agent = match &config {
            Some(c) => format!("Legejo/{} (+{})", env!("CARGO_PKG_VERSION"), c.base),
            None => format!("Legejo/{}", env!("CARGO_PKG_VERSION")),
        };
        Ok(Arc::new(Fed {
            http: net::client(&agent, allow_private)?,
            config,
            wake: tokio::sync::Notify::new(),
            rate: Mutex::new(HashMap::new()),
        }))
    }

    pub fn disabled() -> Arc<Fed> {
        Fed::new(None).expect("client without config")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Off,
    Allowlist,
    Open,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Allowlist => "allowlist",
            Mode::Open => "open",
        }
    }
}

pub async fn setting(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
}

pub async fn mode(state: &AppState) -> Mode {
    match setting(state, "federation_mode").await.as_deref() {
        Some("allowlist") => Mode::Allowlist,
        Some("open") => Mode::Open,
        _ => Mode::Off,
    }
}

/// The config when federation is switched on, else None (404 everywhere).
pub async fn active(state: &AppState) -> Option<FedConfig> {
    let config = state.fed.config.clone()?;
    (mode(state).await != Mode::Off).then_some(config)
}

pub async fn max_epub_bytes(state: &AppState) -> usize {
    let mb: usize = setting(state, "federation_max_epub_mb").await.and_then(|v| v.parse().ok()).unwrap_or(100);
    mb.clamp(1, 200) * 1024 * 1024
}

/// Host part of an IRI ("b.example", with :port if any), lowercased.
pub fn domain_of(iri: &str) -> Option<String> {
    let url = reqwest::Url::parse(iri).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

/// May we talk to this domain? Blocked never; in allowlist mode only what
/// an admin allowed; in open mode everything else. Never ourselves.
pub async fn domain_allowed(state: &AppState, domain: &str) -> bool {
    let Some(config) = active(state).await else { return false };
    if domain.eq_ignore_ascii_case(&config.host) {
        return false;
    }
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM ap_instances WHERE domain = $1")
        .bind(domain.to_ascii_lowercase())
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    match (mode(state).await, status.as_deref()) {
        (_, Some("blocked")) => false,
        (Mode::Allowlist, Some("allowed")) => true,
        (Mode::Allowlist, _) => false,
        (Mode::Open, _) => true,
        (Mode::Off, _) => false,
    }
}

/// A short random id for activities.
pub fn activity_id(config: &FedConfig) -> String {
    format!("{}/ap/activities/{}", config.base, crate::books::new_uuid())
}

/// "Röda rummet" -> "roda-rummet": the [a-z0-9-] form handles are made of.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let mapped = match c {
            'å' | 'ä' | 'à' | 'á' | 'â' => 'a',
            'ö' | 'ø' | 'ó' | 'ò' | 'ô' => 'o',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'ü' | 'ú' | 'ù' => 'u',
            'í' | 'ì' | 'ï' => 'i',
            'ñ' => 'n',
            'ç' => 'c',
            'ß' => 's',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if mapped == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(mapped);
    }
    out.trim_end_matches('-').chars().take(63).collect::<String>().trim_end_matches('-').to_string()
}

pub fn valid_slug(slug: &str) -> bool {
    let b = slug.as_bytes();
    (2..=63).contains(&b.len())
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// Record a refused incoming activity (type, actor, domain and reason, never
/// the body) and log one line. Only the newest 200 rows are kept.
pub async fn reject(state: &AppState, kind: Option<&str>, actor: Option<&str>, reason: &str) {
    let domain = actor.and_then(domain_of);
    tracing::warn!(
        "fed: rejected type={} actor={} domain={} reason={reason}",
        kind.unwrap_or("-"),
        actor.unwrap_or("-"),
        domain.as_deref().unwrap_or("-")
    );
    let _ = sqlx::query(
        "INSERT INTO ap_rejections (at, activity_type, actor, domain, reason) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(crate::db::now_ts())
    .bind(kind)
    .bind(actor)
    .bind(&domain)
    .bind(reason)
    .execute(&state.db)
    .await;
    let _ = sqlx::query(
        "DELETE FROM ap_rejections WHERE id NOT IN (SELECT id FROM ap_rejections ORDER BY id DESC LIMIT 200)",
    )
    .execute(&state.db)
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("markus-Klassiker"), "markus-klassiker");
        assert_eq!(slugify("Åsa / Röda rummet!"), "asa-roda-rummet");
        assert_eq!(slugify("--x--"), "x");
        assert!(valid_slug("alice-klassiker"));
        assert!(!valid_slug("a"));
        assert!(!valid_slug("-a"));
        assert!(!valid_slug("Alice"));
        assert!(!valid_slug("a_b"));
    }

    #[test]
    fn iris() {
        let c = FedConfig { base: "https://a.example".into(), host: "a.example".into(), allow_private: false };
        assert_eq!(c.slug_of("https://a.example/ap/shelves/x-y"), Some("x-y".into()));
        assert_eq!(c.slug_of("https://a.example/ap/shelves/x/inbox"), None);
        assert_eq!(c.slug_of("https://b.example/ap/shelves/x"), None);
        assert_eq!(domain_of("https://B.example:8443/x"), Some("b.example:8443".into()));
    }
}
