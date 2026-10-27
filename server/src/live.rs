//! Connections an admin can set up in the browser: mail (SMTP), the model
//! behind the librarian and the public address. The environment wins: a
//! group set there (LEGEJO_SMTP_HOST, LEGEJO_AI_API_KEY, LEGEJO_PUBLIC_URL)
//! is shown as such and cannot be changed in the browser. Otherwise what the
//! admin saves is kept in `settings` and used at once, without a restart,
//! federation included.
//!
//! Passwords and keys are kept as they are in the database (so they are in
//! its backups), and never handed back: the admin page learns only that one
//! is set.

use crate::auth::AdminUser;
use crate::mail::MailConfig;
use crate::settings::Ai;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::RwLock;

const DEFAULT_AI_BASE: &str = "https://api.openai.com/v1";
const DEFAULT_AI_MODEL: &str = "gpt-5.4-mini";

#[derive(Clone, Default)]
struct Values {
    mail: Option<MailConfig>,
    ai: Option<Ai>,
    public_url: Option<String>,
}

/// Which groups the environment sets.
#[derive(Clone, Copy, Default)]
pub struct FromEnv {
    pub mail: bool,
    pub ai: bool,
    pub public_url: bool,
}

pub struct Live {
    values: RwLock<Values>,
    env: FromEnv,
    /// Search inside the books, as switched on the admin page (fulltext.rs;
    /// LEGEJO_FULLTEXT, when set, wins over it).
    fulltext: std::sync::atomic::AtomicBool,
}

impl Live {
    /// Values as given, all counted as from the environment (tests, and an
    /// instance that has not read the database yet).
    pub fn fixed(mail: Option<MailConfig>, ai: Option<Ai>, public_url: Option<String>) -> Live {
        let env = FromEnv { mail: mail.is_some(), ai: ai.is_some(), public_url: public_url.is_some() };
        Live { values: RwLock::new(Values { mail, ai, public_url }), env, fulltext: false.into() }
    }

    /// The environment first, then what an admin saved.
    pub async fn load(db: &sqlx::AnyPool) -> anyhow::Result<Live> {
        let mail = crate::mail::from_env()?;
        let ai = crate::settings::ai_from_env()?;
        let public_url = crate::settings::var("LEGEJO_PUBLIC_URL")?.map(|u| u.trim().trim_end_matches('/').to_string());
        let env = FromEnv { mail: mail.is_some(), ai: ai.is_some(), public_url: public_url.is_some() };
        let live = Live { values: RwLock::new(Values { mail, ai, public_url }), env, fulltext: false.into() };
        live.reload(db).await?;
        let fulltext: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'fulltext_enabled'").fetch_optional(db).await?;
        live.set_fulltext(fulltext.as_deref() == Some("true"));
        Ok(live)
    }

    /// Read again what the environment does not set.
    pub async fn reload(&self, db: &sqlx::AnyPool) -> anyhow::Result<()> {
        let stored = Stored::read(db).await?;
        let mut values = self.values.write().unwrap();
        if !self.env.mail {
            values.mail = stored.mail_config();
        }
        if !self.env.ai {
            values.ai = stored.ai();
        }
        if !self.env.public_url {
            values.public_url = stored.get("public_url");
        }
        Ok(())
    }

    pub fn fulltext(&self) -> bool {
        self.fulltext.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_fulltext(&self, on: bool) {
        self.fulltext.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn mail(&self) -> Option<MailConfig> {
        self.values.read().unwrap().mail.clone()
    }

    pub fn ai(&self) -> Option<Ai> {
        self.values.read().unwrap().ai.clone()
    }

    /// "https://books.example.org", no trailing slash; None when unknown.
    pub fn public_url(&self) -> Option<String> {
        self.values.read().unwrap().public_url.clone()
    }

    #[cfg(test)]
    pub fn set_mail(&self, mail: Option<MailConfig>) {
        self.values.write().unwrap().mail = mail;
    }

    #[cfg(test)]
    pub fn set_ai(&self, ai: Option<Ai>) {
        self.values.write().unwrap().ai = ai;
    }
}

// ---------------------------------------------------------------- storage

const KEYS: [&str; 10] =
    ["mail.host", "mail.port", "mail.user", "mail.password", "mail.tls", "mail.from", "ai.base_url", "ai.key", "ai.model", "public_url"];

/// The values an admin saved, by key.
struct Stored(std::collections::HashMap<String, String>);

impl Stored {
    async fn read(db: &sqlx::AnyPool) -> Result<Stored, sqlx::Error> {
        let mut map = std::collections::HashMap::new();
        for key in KEYS {
            let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1").bind(key).fetch_optional(db).await?;
            if let Some(v) = value.filter(|v| !v.is_empty()) {
                map.insert(key.to_string(), v);
            }
        }
        Ok(Stored(map))
    }

    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn mail_config(&self) -> Option<MailConfig> {
        let host = self.get("mail.host")?;
        let from = self.get("mail.from")?;
        let port = self.get("mail.port").and_then(|p| p.parse().ok()).unwrap_or(587);
        Some(MailConfig::new(host, port, self.get("mail.user"), self.get("mail.password").unwrap_or_default(), &self.get("mail.tls").unwrap_or_default(), from))
    }

    fn ai(&self) -> Option<Ai> {
        Some(Ai {
            key: self.get("ai.key")?,
            base_url: self.get("ai.base_url").unwrap_or_else(|| DEFAULT_AI_BASE.into()),
            model: self.get("ai.model").unwrap_or_else(|| DEFAULT_AI_MODEL.into()),
        })
    }
}

async fn store(state: &AppState, key: &str, value: Option<&str>) -> Result<(), sqlx::Error> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => {
            sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value")
                .bind(key)
                .bind(v)
                .execute(&state.db)
                .await?;
        }
        None => {
            sqlx::query("DELETE FROM settings WHERE key = $1").bind(key).execute(&state.db).await?;
        }
    }
    Ok(())
}

/// A random id for this instance, the same in every process of it: the
/// test of the public address asks the address for it.
pub async fn instance_id(db: &sqlx::AnyPool) -> Result<String, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = 'instance_id'").fetch_optional(db).await? {
        return Ok(id);
    }
    let id = crate::auth::new_token();
    sqlx::query("INSERT INTO settings (key, value) VALUES ('instance_id', $1) ON CONFLICT (key) DO NOTHING").bind(&id).execute(db).await?;
    sqlx::query_scalar("SELECT value FROM settings WHERE key = 'instance_id'").fetch_one(db).await
}

/// Public: GET /api/instance.
pub async fn instance(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Response> {
    let id = instance_id(&state.db).await.map_err(|e| internal(e.into()))?;
    Ok(Json(json!({ "id": id })))
}

// ---------------------------------------------------------------- admin API

#[derive(Serialize)]
pub struct Connections {
    mail: MailView,
    librarian: LibrarianView,
    public_url: PublicUrlView,
}

#[derive(Serialize)]
struct MailView {
    /// "env", "admin" or "none".
    source: &'static str,
    host: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    password_set: bool,
    tls: Option<&'static str>,
    from: Option<String>,
}

#[derive(Serialize)]
struct LibrarianView {
    source: &'static str,
    base_url: Option<String>,
    model: Option<String>,
    key_set: bool,
}

#[derive(Serialize)]
struct PublicUrlView {
    source: &'static str,
    value: Option<String>,
    /// The address the admin's own request came in on: a suggestion.
    detected: String,
}

fn source(env: bool, set: bool) -> &'static str {
    if env {
        "env"
    } else if set {
        "admin"
    } else {
        "none"
    }
}

/// GET /api/admin/connections
pub async fn get(State(state): State<AppState>, _admin: AdminUser, headers: HeaderMap) -> Result<Json<Connections>, Response> {
    let stored = Stored::read(&state.db).await.map_err(|e| internal(e.into()))?;
    let (mail, ai, public_url) = (state.live.mail(), state.live.ai(), state.live.public_url());
    let env = state.live.env;
    Ok(Json(Connections {
        mail: MailView {
            source: source(env.mail, mail.is_some()),
            host: mail.as_ref().map(|m| m.host().to_string()),
            port: mail.as_ref().map(|m| m.port()),
            user: mail.as_ref().and_then(|m| m.user().map(String::from)),
            password_set: if env.mail { mail.as_ref().is_some_and(|m| m.user().is_some()) } else { stored.get("mail.password").is_some() },
            tls: mail.as_ref().map(|m| m.tls_name()),
            from: mail.as_ref().map(|m| m.from.clone()),
        },
        librarian: LibrarianView {
            source: source(env.ai, ai.is_some()),
            base_url: ai.as_ref().map(|a| a.base_url.clone()).or_else(|| stored.get("ai.base_url")),
            model: ai.as_ref().map(|a| a.model.clone()).or_else(|| stored.get("ai.model")),
            key_set: ai.is_some(),
        },
        public_url: PublicUrlView { source: source(env.public_url, public_url.is_some()), value: public_url, detected: crate::register::base_url(&headers) },
    }))
}

#[derive(Deserialize)]
pub struct MailForm {
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    /// None keeps the saved password; "" removes it.
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    tls: Option<String>,
    #[serde(default)]
    from: Option<String>,
    /// For the test: where to send it. The admin's own address otherwise.
    #[serde(default)]
    to: Option<String>,
}

impl MailForm {
    /// The configuration the form describes; None for an empty host (mail off).
    fn config(&self, saved_password: Option<String>) -> Result<Option<MailConfig>, Response> {
        let host = self.host.trim();
        if host.is_empty() {
            return Ok(None);
        }
        let from = self.from.as_deref().map(str::trim).unwrap_or("");
        if from.parse::<lettre::message::Mailbox>().is_err() {
            return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "from"));
        }
        let password = self.password.clone().or(saved_password).unwrap_or_default();
        let user = self.user.as_deref().map(str::trim).filter(|u| !u.is_empty()).map(String::from);
        let tls = self.tls.as_deref().filter(|t| matches!(*t, "tls" | "starttls")).unwrap_or("starttls");
        Ok(Some(MailConfig::new(host.to_string(), self.port.unwrap_or(587), user, password, tls, from.to_string())))
    }
}

/// PUT /api/admin/connections/mail
pub async fn put_mail(State(state): State<AppState>, admin: AdminUser, headers: HeaderMap, Json(form): Json<MailForm>) -> Result<Json<Connections>, Response> {
    if state.live.env.mail {
        return Err(fail(StatusCode::CONFLICT, "env"));
    }
    form.config(None)?;
    let port = form.port.map(|p| p.to_string());
    let on = !form.host.trim().is_empty();
    let save = async {
        store(&state, "mail.host", Some(&form.host)).await?;
        store(&state, "mail.port", port.as_deref()).await?;
        store(&state, "mail.user", form.user.as_deref()).await?;
        if !on {
            store(&state, "mail.password", None).await?;
        } else if let Some(password) = &form.password {
            store(&state, "mail.password", Some(password)).await?;
        }
        store(&state, "mail.tls", form.tls.as_deref()).await?;
        store(&state, "mail.from", form.from.as_deref()).await
    };
    save.await.map_err(|e| internal(e.into()))?;
    changed(&state, &admin, "mail", json!({ "host": form.host.trim(), "on": on })).await?;
    get(State(state), admin, headers).await
}

/// POST /api/admin/connections/mail/test: send a mail with what the form
/// says, before it is saved. The answer says what the server said.
pub async fn test_mail(State(state): State<AppState>, admin: AdminUser, Json(form): Json<MailForm>) -> Result<Json<serde_json::Value>, Response> {
    let config = if state.live.env.mail {
        state.live.mail()
    } else {
        let saved = Stored::read(&state.db).await.map_err(|e| internal(e.into()))?.get("mail.password");
        form.config(saved)?
    };
    let Some(config) = config else {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "host"));
    };
    let own: Option<String> = sqlx::query_scalar("SELECT email FROM users WHERE id = $1").bind(admin.0.id).fetch_one(&state.db).await.map_err(|e| internal(e.into()))?;
    let Some(to) = form.to.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(String::from).or(own) else {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "to"));
    };
    let html = "<p>This is a test from Legejo. Mail works: invitations, password resets and \
                registration can go out from here.</p>";
    Ok(Json(match crate::mail::send(&config, &to, "Legejo: test mail", html).await {
        Ok(()) => json!({ "ok": true, "to": to }),
        Err(e) => json!({ "ok": false, "to": to, "error": format!("{e:#}") }),
    }))
}

#[derive(Deserialize)]
pub struct LibrarianForm {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    model: Option<String>,
    /// None keeps the saved key; "" removes it (and the librarian with it).
    #[serde(default)]
    key: Option<String>,
}

impl LibrarianForm {
    fn ai(&self, saved_key: Option<String>) -> Result<Option<Ai>, Response> {
        let base_url = self.base_url.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(DEFAULT_AI_BASE);
        if !matches!(reqwest::Url::parse(base_url).map(|u| u.scheme().to_string()).as_deref(), Ok("https" | "http")) {
            return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "base_url"));
        }
        let key = match &self.key {
            Some(k) => Some(k.trim().to_string()).filter(|k| !k.is_empty()),
            None => saved_key,
        };
        Ok(key.map(|key| Ai {
            key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model: self.model.as_deref().map(str::trim).filter(|m| !m.is_empty()).unwrap_or(DEFAULT_AI_MODEL).to_string(),
        }))
    }
}

/// PUT /api/admin/connections/librarian
pub async fn put_librarian(State(state): State<AppState>, admin: AdminUser, headers: HeaderMap, Json(form): Json<LibrarianForm>) -> Result<Json<Connections>, Response> {
    if state.live.env.ai {
        return Err(fail(StatusCode::CONFLICT, "env"));
    }
    form.ai(None)?;
    let save = async {
        store(&state, "ai.base_url", form.base_url.as_deref().map(|u| u.trim().trim_end_matches('/'))).await?;
        store(&state, "ai.model", form.model.as_deref()).await?;
        if let Some(key) = &form.key {
            store(&state, "ai.key", Some(key)).await?;
        }
        Ok::<_, sqlx::Error>(())
    };
    save.await.map_err(|e| internal(e.into()))?;
    changed(&state, &admin, "librarian", json!({ "base_url": form.base_url, "model": form.model })).await?;
    get(State(state), admin, headers).await
}

/// POST /api/admin/connections/librarian/test: one short question to the
/// model with what the form says.
pub async fn test_librarian(State(state): State<AppState>, _admin: AdminUser, Json(form): Json<LibrarianForm>) -> Result<Json<serde_json::Value>, Response> {
    let ai = if state.live.env.ai {
        state.live.ai()
    } else {
        let saved = Stored::read(&state.db).await.map_err(|e| internal(e.into()))?.get("ai.key");
        form.ai(saved)?
    };
    let Some(ai) = ai else {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "key"));
    };
    let request = json!({ "model": ai.model, "messages": [{ "role": "user", "content": "Answer with the single word: ready" }] });
    let response = state
        .http
        .post(format!("{}/chat/completions", ai.base_url))
        .bearer_auth(&ai.key)
        .timeout(std::time::Duration::from_secs(30))
        .json(&request)
        .send()
        .await;
    Ok(Json(match response {
        Err(e) => json!({ "ok": false, "error": format!("no answer: {e}") }),
        Ok(r) => {
            let status = r.status();
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            if status.is_success() {
                json!({ "ok": true, "model": body["model"].as_str().unwrap_or(&ai.model) })
            } else {
                json!({ "ok": false, "error": format!("HTTP {status}: {}", body["error"]["message"].as_str().unwrap_or("no reason given")) })
            }
        }
    }))
}

#[derive(Deserialize)]
pub struct PublicUrlForm {
    #[serde(default)]
    url: String,
}

/// "https://books.example.org" from what was typed; Err for anything that is
/// not an http(s) address with nothing after the host.
fn clean_url(raw: &str) -> Result<Option<String>, Response> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Ok(None);
    }
    if !raw.contains("://") {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "scheme"));
    }
    let url = reqwest::Url::parse(raw).map_err(|_| fail(StatusCode::UNPROCESSABLE_ENTITY, "url"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() || url.path() != "/" || url.query().is_some() {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "url"));
    }
    Ok(Some(raw.to_string()))
}

/// PUT /api/admin/connections/public-url
pub async fn put_public_url(State(state): State<AppState>, admin: AdminUser, headers: HeaderMap, Json(form): Json<PublicUrlForm>) -> Result<Json<Connections>, Response> {
    if state.live.env.public_url {
        return Err(fail(StatusCode::CONFLICT, "env"));
    }
    let url = clean_url(&form.url)?;
    store(&state, "public_url", url.as_deref()).await.map_err(|e| internal(e.into()))?;
    changed(&state, &admin, "public_url", json!({ "url": url })).await?;
    get(State(state), admin, headers).await
}

/// POST /api/admin/connections/public-url/test: ask the address for this
/// instance's id, so a wrong address (or one that leads elsewhere) shows.
pub async fn test_public_url(State(state): State<AppState>, _admin: AdminUser, Json(form): Json<PublicUrlForm>) -> Result<Json<serde_json::Value>, Response> {
    let Some(url) = clean_url(&form.url)? else {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, "url"));
    };
    let ours = instance_id(&state.db).await.map_err(|e| internal(e.into()))?;
    let answer = state.http.get(format!("{url}/api/instance")).timeout(std::time::Duration::from_secs(10)).send().await;
    let https = url.starts_with("https://");
    Ok(Json(match answer {
        Err(e) => json!({ "ok": false, "error": format!("no answer: {e}") }),
        Ok(r) if !r.status().is_success() => json!({ "ok": false, "error": format!("HTTP {}", r.status()) }),
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(body) if body["id"].as_str() == Some(ours.as_str()) => json!({ "ok": true, "https": https }),
            _ => json!({ "ok": false, "error": "another server answered at that address" }),
        },
    }))
}

async fn changed(state: &AppState, admin: &AdminUser, what: &str, details: serde_json::Value) -> Result<(), Response> {
    state.live.reload(&state.db).await.map_err(internal)?;
    if what == "public_url" {
        // Federation names the instance by it, from now on.
        state.fed.set_config(crate::fed::FedConfig::from_url(state.live.public_url()));
    }
    crate::audit::log(state, crate::audit::by(&admin.0), "admin.connections", json!({ "what": what, "details": details })).await;
    Ok(())
}

fn fail(status: StatusCode, field: &str) -> Response {
    (status, Json(json!({ "error": field }))).into_response()
}

fn internal(e: anyhow::Error) -> Response {
    tracing::error!("internal error: {e:#}");
    fail(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}
