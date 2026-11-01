//! Operator settings from the environment, read once at startup.
//!
//! Secrets may come from files: for any variable NAME, NAME_FILE names a
//! file holding the value (Docker/Kubernetes secrets), used when NAME itself
//! is unset.

use std::path::PathBuf;

/// NAME, else the contents of the file NAME_FILE (one trailing newline
/// dropped), else None. Empty counts as unset. An unreadable NAME_FILE is
/// an error: the operator asked for it.
fn lookup(name: &str) -> anyhow::Result<Option<String>> {
    if let Ok(v) = std::env::var(name) {
        if !v.is_empty() {
            return Ok(Some(v));
        }
    }
    let file_var = format!("{name}_FILE");
    match std::env::var(&file_var) {
        Ok(path) if !path.is_empty() => {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("{file_var}={path}: cannot read the file: {e}"))?;
            let v = raw.strip_suffix('\n').map(|s| s.strip_suffix('\r').unwrap_or(s)).unwrap_or(&raw);
            Ok((!v.is_empty()).then(|| v.to_string()))
        }
        _ => Ok(None),
    }
}

/// A variable, or the contents of the file named by NAME_FILE.
pub fn var(name: &str) -> anyhow::Result<Option<String>> {
    lookup(name)
}

/// Like `var`, for callers that cannot fail: an unreadable NAME_FILE is
/// logged and counts as unset.
pub fn var_lossy(name: &str) -> Option<String> {
    var(name).unwrap_or_else(|e| {
        tracing::error!("{e:#}");
        None
    })
}

/// "1", "true", "yes", "on" (any case).
pub fn flag(name: &str) -> anyhow::Result<bool> {
    Ok(var(name)?.is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on")))
}

fn number(name: &str, default: i64) -> anyhow::Result<i64> {
    match var(name)? {
        None => Ok(default),
        Some(v) => v
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or_else(|| anyhow::anyhow!("{name} must be a whole number ≥ 0, not {v:?}")),
    }
}

/// Login throttling (ratelimit.rs).
#[derive(Clone, Debug)]
pub struct LoginLimits {
    /// Failed logins per username (and IP, when known) within the window
    /// before further attempts get 429. 0 = no throttling.
    pub max_failures: i64,
    /// Failures from one IP, over all usernames; only with `ip_header`.
    pub max_failures_ip: i64,
    pub window_minutes: i64,
    /// Request header carrying the client's IP (lowercase), set by the
    /// reverse proxy, e.g. X-Real-IP or X-Forwarded-For.
    pub ip_header: Option<String>,
}

/// The watched import folder (importdir.rs).
#[derive(Clone, Debug)]
pub struct ImportDir {
    pub dir: PathBuf,
    /// Username that receives the books; None = the oldest admin.
    pub user: Option<String>,
    pub interval_secs: u64,
    /// Delete imported files instead of moving them to imported/.
    pub delete: bool,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub limits: LoginLimits,
    pub max_upload_bytes: usize,
    /// /metrics: off, open, or behind a bearer token.
    pub metrics: Metrics,
    pub import: Option<ImportDir>,
    /// The same for audiobooks (audioimport.rs). None unless
    /// LEGEJO_AUDIOBOOK_IMPORT_DIR is set.
    pub audio_import: Option<ImportDir>,
    /// Mark cookies Secure, as LEGEJO_SECURE_COOKIES says; unset, it follows
    /// the public address as it is now (auth::secure_cookies).
    pub secure_cookies: Option<bool>,
    /// Serve the read-only MCP endpoint (mcp.rs). Off unless LEGEJO_MCP=true.
    pub mcp: bool,
    /// Keep the text of the books for searching inside them (fulltext.rs):
    /// LEGEJO_FULLTEXT when it is set. None leaves it to the admin page
    /// (live.rs), where it is off until switched on: it takes room in the
    /// database.
    pub fulltext: Option<bool>,
    /// Kobo's own services, reached for a device that keeps the Kobo store
    /// beside Legejo (kobostore.rs). Fixed; tests point them elsewhere.
    pub kobo: KoboServices,
    /// The dictionaries the reader looks words up in (dictionary.rs).
    /// Fixed; tests point them elsewhere.
    pub dictionary: DictionaryServices,
    /// Suggest shelves on other instances to follow (fed::remote). On
    /// unless LEGEJO_SHELF_SUGGESTIONS=off.
    pub shelf_suggestions: bool,
    /// Log all that is said with a Kobo (kobotrace.rs). Off unless
    /// LEGEJO_KOBO_TRACE=true.
    pub kobo_trace: bool,
}

#[derive(Clone, Debug)]
pub struct DictionaryServices {
    /// Reta Vortaro, for Esperanto.
    pub revo: String,
    /// The English Wiktionary, for every language.
    pub wiktionary: String,
}

impl Default for DictionaryServices {
    fn default() -> Self {
        DictionaryServices { revo: "https://reta-vortaro.de".into(), wiktionary: "https://en.wiktionary.org".into() }
    }
}

#[derive(Clone, Debug)]
pub struct KoboServices {
    /// The store API: initialization, library sync, products.
    pub store: String,
    /// The reading services: highlights and notes.
    pub reading: String,
    /// Covers of books from the store.
    pub images: String,
}

impl Default for KoboServices {
    fn default() -> Self {
        KoboServices {
            store: "https://storeapi.kobo.com".into(),
            reading: "https://readingservices.kobo.com".into(),
            images: "https://cdn.kobo.com/book-images".into(),
        }
    }
}

/// A chat-completions endpoint of the OpenAI kind, which many providers and
/// local model servers offer.
#[derive(Clone)]
pub struct Ai {
    pub base_url: String,
    pub key: String,
    pub model: String,
}

// The key stays out of anything that prints the settings.
impl std::fmt::Debug for Ai {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ai").field("base_url", &self.base_url).field("model", &self.model).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Metrics {
    Off,
    Open,
    Token(String),
}

impl Default for Settings {
    /// What an operator gets without setting anything.
    fn default() -> Self {
        Settings {
            limits: LoginLimits { max_failures: 10, max_failures_ip: 50, window_minutes: 15, ip_header: None },
            max_upload_bytes: 200 * 1024 * 1024,
            metrics: Metrics::Off,
            import: None,
            audio_import: None,
            secure_cookies: None,
            mcp: false,
            fulltext: None,
            kobo: KoboServices::default(),
            dictionary: DictionaryServices::default(),
            shelf_suggestions: true,
            kobo_trace: false,
        }
    }
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Settings> {
        let d = Settings::default();
        let limits = LoginLimits {
            max_failures: number("LEGEJO_LOGIN_MAX_FAILURES", d.limits.max_failures)?,
            max_failures_ip: number("LEGEJO_LOGIN_MAX_FAILURES_IP", d.limits.max_failures_ip)?,
            window_minutes: number("LEGEJO_LOGIN_WINDOW_MINUTES", d.limits.window_minutes)?.max(1),
            ip_header: var("LEGEJO_CLIENT_IP_HEADER")?.map(|h| h.trim().to_lowercase()),
        };
        let max_upload_mb = number("LEGEJO_MAX_UPLOAD_MB", 200)?.max(1);
        let metrics = match (var("LEGEJO_METRICS_TOKEN")?, flag("LEGEJO_METRICS")?) {
            (Some(token), _) => Metrics::Token(token),
            (None, true) => Metrics::Open,
            (None, false) => Metrics::Off,
        };
        let import = match var("LEGEJO_IMPORT_DIR")? {
            None => None,
            Some(dir) => Some(ImportDir {
                dir: PathBuf::from(dir),
                user: var("LEGEJO_IMPORT_USER")?,
                interval_secs: number("LEGEJO_IMPORT_INTERVAL", 60)?.max(5) as u64,
                delete: flag("LEGEJO_IMPORT_DELETE")?,
            }),
        };
        let audio_import = match var("LEGEJO_AUDIOBOOK_IMPORT_DIR")? {
            None => None,
            Some(dir) => Some(ImportDir {
                dir: PathBuf::from(dir),
                user: var("LEGEJO_AUDIOBOOK_IMPORT_USER")?,
                interval_secs: number("LEGEJO_AUDIOBOOK_IMPORT_INTERVAL", 60)?.max(5) as u64,
                delete: flag("LEGEJO_AUDIOBOOK_IMPORT_DELETE")?,
            }),
        };
        let secure_cookies = match var("LEGEJO_SECURE_COOKIES")? {
            Some(_) => Some(flag("LEGEJO_SECURE_COOKIES")?),
            None => None,
        };
        let mcp = flag("LEGEJO_MCP")?;
        let fulltext = match var("LEGEJO_FULLTEXT")? {
            Some(_) => Some(flag("LEGEJO_FULLTEXT")?),
            None => None,
        };
        Ok(Settings { limits, max_upload_bytes: max_upload_mb as usize * 1024 * 1024, metrics, import, audio_import, secure_cookies, mcp, fulltext, kobo: d.kobo, dictionary: d.dictionary, shelf_suggestions: shelf_suggestions()?, kobo_trace: flag("LEGEJO_KOBO_TRACE")? })
    }
}

/// The language model behind the librarian from the environment; None
/// unless LEGEJO_AI_API_KEY is set (an admin may then set it up in the
/// browser, live.rs).
pub fn ai_from_env() -> anyhow::Result<Option<Ai>> {
    let Some(key) = var("LEGEJO_AI_API_KEY")? else {
        return Ok(None);
    };
    Ok(Some(Ai {
        key: key.trim().to_string(),
        base_url: var("LEGEJO_AI_BASE_URL")?
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
        model: var("LEGEJO_AI_MODEL")?.map(|m| m.trim().to_string()).unwrap_or_else(|| "gpt-5.4-mini".to_string()),
    }))
}

/// LEGEJO_SHELF_SUGGESTIONS: on unless "off" (or "false", "0", "no").
fn shelf_suggestions() -> anyhow::Result<bool> {
    Ok(!var("LEGEJO_SHELF_SUGGESTIONS")?.is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "off" | "false" | "0" | "no")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_can_come_from_files() {
        let dir = std::env::temp_dir().join(format!("legejo-env-{}", crate::books::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("secret");
        std::fs::write(&file, "s3cret\n").unwrap();
        // Unique names: tests run in parallel in one process.
        std::env::set_var("LEGEJO_TEST_A_FILE", &file);
        assert_eq!(var("LEGEJO_TEST_A").unwrap().as_deref(), Some("s3cret"));
        // The plain variable wins.
        std::env::set_var("LEGEJO_TEST_A", "direct");
        assert_eq!(var("LEGEJO_TEST_A").unwrap().as_deref(), Some("direct"));
        std::env::set_var("LEGEJO_TEST_B_FILE", dir.join("missing"));
        assert!(var("LEGEJO_TEST_B").is_err());
        assert_eq!(var("LEGEJO_TEST_C").unwrap(), None);
        std::env::set_var("LEGEJO_TEST_D", "x");
        assert!(number("LEGEJO_TEST_D", 1).is_err());
    }
}
