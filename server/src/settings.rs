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
    /// Mark cookies Secure. On by default when LEGEJO_PUBLIC_URL is https.
    pub secure_cookies: bool,
    /// Serve the read-only MCP endpoint (mcp.rs). Off unless LEGEJO_MCP=true.
    pub mcp: bool,
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
            secure_cookies: false,
            mcp: false,
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
        let secure_cookies = match var("LEGEJO_SECURE_COOKIES")? {
            Some(_) => flag("LEGEJO_SECURE_COOKIES")?,
            None => var("LEGEJO_PUBLIC_URL")?.is_some_and(|u| u.trim().starts_with("https://")),
        };
        let mcp = flag("LEGEJO_MCP")?;
        Ok(Settings { limits, max_upload_bytes: max_upload_mb as usize * 1024 * 1024, metrics, import, secure_cookies, mcp })
    }
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
