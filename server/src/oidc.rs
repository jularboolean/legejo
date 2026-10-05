//! Optional login through an OpenID Connect provider (Pocket ID, Authentik,
//! Keycloak, Forgejo, etc.).
//!
//! Disabled unless `LEGEJO_OIDC_ISSUER` is set. It is offered alongside the
//! password form, not instead of it; password login, OPDS, Kobo and KOReader
//! are unaffected. Configuration:
//!
//!   LEGEJO_OIDC_ISSUER         e.g. https://id.example.org
//!   LEGEJO_OIDC_CLIENT_ID
//!   LEGEJO_OIDC_CLIENT_SECRET
//!   LEGEJO_OIDC_NAME           button label, default "SSO"
//!   LEGEJO_OIDC_AUTO_CREATE    "true": unknown people get an account
//!   LEGEJO_OIDC_TRUST_EMAIL    "true": link by email even without email_verified
//!   LEGEJO_OIDC_ADMIN_GROUP    members of this group become admins (never revoked)
//!   LEGEJO_OIDC_SCOPES         default "openid profile email" (+ "groups")
//!
//! The secret may come from a file: LEGEJO_OIDC_CLIENT_SECRET_FILE.
//!
//! The redirect URI to register at the provider is
//! `<LEGEJO_PUBLIC_URL>/api/auth/oidc/callback`.
//!
//! Authorization code flow with PKCE, state and nonce. The flow state is kept
//! in a short-lived HttpOnly cookie rather than in process memory, so a login
//! survives being split across server processes (e.g. during a rolling
//! deploy). The ID token is verified against the provider's JWKS (RS256).
//!
//! An identity maps to a Legejo user by an existing link (issuer + subject in
//! `oidc_identities`), else by a matching verified email, else by creating a
//! new account when auto-create is on. A logged-in user can also link their
//! account from the account page.

use crate::auth::{self, AuthUser};
use crate::db::{now_ts, DbFlag};
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::signature::Verifier;
use rsa::{BigUint, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const FLOW_COOKIE: &str = "legejo_oidc";
const FLOW_MINUTES: i64 = 10;
const CALLBACK_PATH: &str = "/api/auth/oidc/callback";

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub name: String,
    pub auto_create: bool,
    pub trust_email: bool,
    pub admin_group: Option<String>,
    pub scopes: String,
}

impl OidcConfig {
    /// None when OIDC is not configured; an error when it is half-configured.
    pub fn from_env() -> anyhow::Result<Option<OidcConfig>> {
        // Values may come from NAME_FILE. Resolve the secret strictly first so
        // an unreadable file aborts startup instead of being silently ignored.
        crate::settings::var("LEGEJO_OIDC_CLIENT_SECRET")?;
        let var = |k: &str| crate::settings::var_lossy(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let Some(issuer) = var("LEGEJO_OIDC_ISSUER") else {
            return Ok(None);
        };
        let client_id = var("LEGEJO_OIDC_CLIENT_ID")
            .ok_or_else(|| anyhow::anyhow!("LEGEJO_OIDC_ISSUER is set but LEGEJO_OIDC_CLIENT_ID is missing"))?;
        let client_secret = var("LEGEJO_OIDC_CLIENT_SECRET")
            .ok_or_else(|| anyhow::anyhow!("LEGEJO_OIDC_ISSUER is set but LEGEJO_OIDC_CLIENT_SECRET is missing"))?;
        let flag = |k: &str| var(k).is_some_and(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"));
        let admin_group = var("LEGEJO_OIDC_ADMIN_GROUP");
        let scopes = var("LEGEJO_OIDC_SCOPES").unwrap_or_else(|| {
            if admin_group.is_some() { "openid profile email groups" } else { "openid profile email" }.into()
        });
        Ok(Some(OidcConfig {
            issuer: issuer.trim_end_matches('/').to_string(),
            client_id,
            client_secret,
            name: var("LEGEJO_OIDC_NAME").unwrap_or_else(|| "SSO".into()),
            auto_create: flag("LEGEJO_OIDC_AUTO_CREATE"),
            trust_email: flag("LEGEJO_OIDC_TRUST_EMAIL"),
            admin_group,
            scopes,
        }))
    }
}

#[derive(Deserialize, Clone)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    userinfo_endpoint: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
pub(crate) struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize, Clone)]
struct Jwk {
    kty: String,
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
    #[serde(rename = "use")]
    use_: Option<String>,
}

/// The provider's metadata and keys, fetched on first use and refreshed
/// hourly or when a token names a key we have not seen.
struct Cached {
    fetched: Instant,
    discovery: Discovery,
    jwks: Jwks,
}

pub struct Oidc {
    pub config: OidcConfig,
    cache: RwLock<Option<Cached>>,
}

impl Oidc {
    pub fn new(config: Option<OidcConfig>) -> Option<Arc<Oidc>> {
        config.map(|config| Arc::new(Oidc { config, cache: RwLock::new(None) }))
    }

    async fn discovery(&self, http: &reqwest::Client, refresh_keys: bool) -> anyhow::Result<(Discovery, Jwks)> {
        if !refresh_keys {
            if let Some(c) = self.cache.read().await.as_ref() {
                if c.fetched.elapsed() < Duration::from_secs(3600) {
                    return Ok((c.discovery.clone(), c.jwks.clone()));
                }
            }
        }
        let url = format!("{}/.well-known/openid-configuration", self.config.issuer);
        let discovery: Discovery = http.get(&url).send().await?.error_for_status()?.json().await?;
        if discovery.issuer.trim_end_matches('/') != self.config.issuer {
            anyhow::bail!("the provider calls itself {}, not {}", discovery.issuer, self.config.issuer);
        }
        let jwks: Jwks = http.get(&discovery.jwks_uri).send().await?.error_for_status()?.json().await?;
        *self.cache.write().await = Some(Cached { fetched: Instant::now(), discovery: discovery.clone(), jwks: jwks.clone() });
        Ok((discovery, jwks))
    }
}

/// OIDC info for the login and account pages.
#[derive(Serialize)]
pub struct AuthConfig {
    oidc: Option<OidcInfo>,
}

#[derive(Serialize)]
struct OidcInfo {
    name: String,
}

pub async fn config(State(state): State<AppState>) -> Json<AuthConfig> {
    Json(AuthConfig { oidc: state.oidc.as_ref().map(|o| OidcInfo { name: o.config.name.clone() }) })
}

fn random_hex(bytes: usize) -> String {
    use rand::RngCore;
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn redirect_uri(state: &AppState, headers: &HeaderMap) -> String {
    format!("{}{CALLBACK_PATH}", crate::invite::base(state, headers))
}

/// Flow state, held in a browser cookie between start and callback.
struct Flow {
    state: String,
    verifier: String,
    nonce: String,
    /// Started from the account page by a logged-in user.
    link: bool,
}

impl Flow {
    fn encode(&self) -> String {
        format!("{}.{}.{}.{}", self.state, self.verifier, self.nonce, if self.link { "l" } else { "n" })
    }

    fn decode(s: &str) -> Option<Flow> {
        let mut it = s.split('.');
        let flow = Flow {
            state: it.next()?.to_string(),
            verifier: it.next()?.to_string(),
            nonce: it.next()?.to_string(),
            link: it.next()? == "l",
        };
        (it.next().is_none() && !flow.state.is_empty()).then_some(flow)
    }
}

fn flow_cookie(value: String, minutes: i64, secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::new(FLOW_COOKIE, value);
    cookie.set_path("/api/auth/oidc");
    cookie.set_http_only(true);
    cookie.set_secure(secure);
    // Lax suffices: the provider's redirect back is a top-level GET.
    cookie.set_same_site(SameSite::Lax);
    cookie.set_max_age(time::Duration::minutes(minutes));
    cookie
}

fn not_configured() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "OIDC is not configured" }))).into_response()
}

#[derive(Deserialize)]
pub struct StartQuery {
    link: Option<String>,
}

/// GET /api/auth/oidc/start[?link=1]: redirects to the provider.
pub async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(q): Query<StartQuery>,
) -> Response {
    let Some(oidc) = state.oidc.clone() else {
        return not_configured();
    };
    let link = q.link.is_some();
    let back = if link { "/account" } else { "/login" };
    let (discovery, _) = match oidc.discovery(&state.http, false).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("OIDC: could not reach the provider: {e:#}");
            return Redirect::to(&format!("{back}?oidc=unavailable")).into_response();
        }
    };
    let flow = Flow { state: random_hex(16), verifier: random_hex(32), nonce: random_hex(16), link };
    let mut url = match url::Url::parse(&discovery.authorization_endpoint) {
        Ok(u) => u,
        Err(_) => return Redirect::to(&format!("{back}?oidc=unavailable")).into_response(),
    };
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &oidc.config.client_id)
        .append_pair("redirect_uri", &redirect_uri(&state, &headers))
        .append_pair("scope", &oidc.config.scopes)
        .append_pair("state", &flow.state)
        .append_pair("nonce", &flow.nonce)
        .append_pair("code_challenge", &pkce_challenge(&flow.verifier))
        .append_pair("code_challenge_method", "S256");
    (jar.add(flow_cookie(flow.encode(), FLOW_MINUTES, state.settings.secure_cookies)), Redirect::to(url.as_str())).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: Option<String>,
}

/// The ID-token claims Legejo uses.
#[derive(Deserialize, Debug, Default)]
pub(crate) struct Claims {
    iss: String,
    sub: String,
    aud: Value,
    exp: i64,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<Value>,
    preferred_username: Option<String>,
    name: Option<String>,
    groups: Option<Value>,
}

/// Login failure reason, passed to the web app as `?oidc=<code>`.
#[derive(Debug, PartialEq)]
enum Failure {
    /// Bad state, expired flow, provider error, token that does not verify.
    Failed,
    /// No account matched and auto-create is off.
    NoAccount,
    /// The identity is already linked to another account.
    AlreadyLinked,
    Internal,
}

impl Failure {
    fn code(&self) -> &'static str {
        match self {
            Failure::Failed => "failed",
            Failure::NoAccount => "no_account",
            Failure::AlreadyLinked => "already_linked",
            Failure::Internal => "error",
        }
    }
}

fn internal(e: impl std::fmt::Display) -> Failure {
    tracing::error!("OIDC: {e}");
    Failure::Internal
}

/// GET /api/auth/oidc/callback: redirect target after the provider.
pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = state.oidc.clone() else {
        return not_configured();
    };
    let flow = jar.get(FLOW_COOKIE).and_then(|c| Flow::decode(c.value()));
    let jar = jar.remove(flow_cookie(String::new(), 0, state.settings.secure_cookies));
    let link = flow.as_ref().is_some_and(|f| f.link);
    let back = if link { "/account" } else { "/login" };

    // For a link flow, the user whose session started it.
    let linking_user = match link {
        true => match jar.get(auth::SESSION_COOKIE) {
            Some(c) => session_user(&state, c.value()).await,
            None => None,
        },
        false => None,
    };

    match finish(&state, &oidc, &headers, flow, linking_user, q).await {
        Ok(Done::LoggedIn(user_id)) => match new_session(&state, user_id).await {
            Ok(token) => (jar.add(auth::session_cookie(token, auth::SESSION_DAYS, state.settings.secure_cookies)), Redirect::to("/")).into_response(),
            Err(_) => (jar, Redirect::to(&format!("{back}?oidc=error"))).into_response(),
        },
        Ok(Done::Linked) => (jar, Redirect::to("/account?oidc=linked")).into_response(),
        Err(f) => (jar, Redirect::to(&format!("{back}?oidc={}", f.code()))).into_response(),
    }
}

enum Done {
    LoggedIn(i64),
    Linked,
}

async fn finish(
    state: &AppState,
    oidc: &Oidc,
    headers: &HeaderMap,
    flow: Option<Flow>,
    linking_user: Option<(i64, String)>,
    q: CallbackQuery,
) -> Result<Done, Failure> {
    let flow = flow.ok_or(Failure::Failed)?;
    if let Some(e) = &q.error {
        tracing::info!("OIDC: the provider answered {e}");
        return Err(Failure::Failed);
    }
    if q.state.as_deref() != Some(flow.state.as_str()) {
        return Err(Failure::Failed);
    }
    let code = q.code.ok_or(Failure::Failed)?;
    if flow.link && linking_user.is_none() {
        return Err(Failure::Failed);
    }

    let (discovery, jwks) = oidc.discovery(&state.http, false).await.map_err(|e| {
        tracing::warn!("OIDC: could not reach the provider: {e:#}");
        Failure::Failed
    })?;
    let tokens: TokenResponse = async {
        state
            .http
            .post(&discovery.token_endpoint)
            .basic_auth(form_encode(&oidc.config.client_id), Some(form_encode(&oidc.config.client_secret)))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", redirect_uri(state, headers).as_str()),
                ("code_verifier", flow.verifier.as_str()),
                ("client_id", oidc.config.client_id.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }
    .await
    .map_err(|e: reqwest::Error| {
        tracing::warn!("OIDC: token exchange failed: {e}");
        Failure::Failed
    })?;

    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut claims = match verify_id_token(&tokens.id_token, &jwks, &discovery.issuer, &oidc.config.client_id, &flow.nonce, now) {
        Err(TokenError::UnknownKey) => {
            // The provider may have rotated its keys.
            let (_, jwks) = oidc.discovery(&state.http, true).await.map_err(|_| Failure::Failed)?;
            verify_id_token(&tokens.id_token, &jwks, &discovery.issuer, &oidc.config.client_id, &flow.nonce, now)
        }
        other => other,
    }
    .map_err(|e| {
        tracing::warn!("OIDC: rejected ID token: {e:?}");
        Failure::Failed
    })?;

    // Some providers keep email and groups out of the ID token.
    if claims.email.is_none() || (oidc.config.admin_group.is_some() && claims.groups.is_none()) {
        if let (Some(endpoint), Some(access)) = (&discovery.userinfo_endpoint, &tokens.access_token) {
            if let Ok(info) = fetch_userinfo(&state.http, endpoint, access).await {
                // Userinfo must describe the same person (OIDC Core 5.3.2).
                if info.get("sub").and_then(Value::as_str) == Some(claims.sub.as_str()) {
                    merge_userinfo(&mut claims, &info);
                }
            }
        }
    }

    resolve(state, oidc, &claims, linking_user).await
}

async fn fetch_userinfo(http: &reqwest::Client, endpoint: &str, access: &str) -> reqwest::Result<Value> {
    http.get(endpoint).bearer_auth(access).send().await?.error_for_status()?.json().await
}

fn merge_userinfo(claims: &mut Claims, info: &Value) {
    let s = |k: &str| info.get(k).and_then(Value::as_str).map(str::to_string);
    if claims.email.is_none() {
        claims.email = s("email");
        claims.email_verified = info.get("email_verified").cloned();
    }
    claims.preferred_username = claims.preferred_username.take().or_else(|| s("preferred_username"));
    claims.name = claims.name.take().or_else(|| s("name"));
    if claims.groups.is_none() {
        claims.groups = info.get("groups").cloned();
    }
}

/// client_secret_basic requires id and secret to be form-encoded before
/// base64 (RFC 6749 2.3.1). A no-op for typical alphanumeric values.
fn form_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Maps verified claims to a Legejo user: linked identity, then verified
/// email, then a new account.
async fn resolve(
    state: &AppState,
    oidc: &Oidc,
    claims: &Claims,
    linking_user: Option<(i64, String)>,
) -> Result<Done, Failure> {
    let issuer = claims.iss.trim_end_matches('/');
    let linked: Option<i64> = sqlx::query_scalar("SELECT user_id FROM oidc_identities WHERE issuer = $1 AND subject = $2")
        .bind(issuer)
        .bind(&claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(internal)?;

    if let Some((user_id, username)) = linking_user {
        return match linked {
            Some(existing) if existing == user_id => Ok(Done::Linked),
            Some(_) => Err(Failure::AlreadyLinked),
            None => {
                link_identity(state, issuer, &claims.sub, user_id).await?;
                crate::audit::log(state, Some((user_id, &username)), "account.oidc_linked", json!({ "provider": oidc.config.name })).await;
                Ok(Done::Linked)
            }
        };
    }

    let user_id = match linked {
        Some(id) => id,
        None => match by_email(state, oidc, claims).await? {
            Some((id, username)) => {
                link_identity(state, issuer, &claims.sub, id).await?;
                crate::audit::log(state, Some((id, &username)), "account.oidc_linked", json!({ "provider": oidc.config.name })).await;
                id
            }
            None if oidc.config.auto_create => create_user(state, oidc, claims).await?,
            None => return Err(Failure::NoAccount),
        },
    };

    if let Some(group) = &oidc.config.admin_group {
        if in_group(claims, group) {
            sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2 AND is_admin = 0")
                .bind(DbFlag::from(true))
                .bind(user_id)
                .execute(&state.db)
                .await
                .map_err(internal)?;
        }
    }
    Ok(Done::LoggedIn(user_id))
}

fn email_is_verified(claims: &Claims) -> bool {
    match &claims.email_verified {
        Some(Value::Bool(b)) => *b,
        // Some providers send the string "true".
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
}

/// An existing, verified Legejo account with the same email.
async fn by_email(state: &AppState, oidc: &Oidc, claims: &Claims) -> Result<Option<(i64, String)>, Failure> {
    let Some(email) = claims.email.as_deref().map(str::trim).filter(|e| e.contains('@')) else {
        return Ok(None);
    };
    if !(oidc.config.trust_email || email_is_verified(claims)) {
        return Ok(None);
    }
    sqlx::query_as(
        "SELECT id, username FROM users
         WHERE email IS NOT NULL AND LOWER(email) = LOWER($1)
           AND verify_token IS NULL AND invite_token IS NULL",
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)
}

async fn link_identity(state: &AppState, issuer: &str, subject: &str, user_id: i64) -> Result<(), Failure> {
    sqlx::query("INSERT INTO oidc_identities (issuer, subject, user_id, created_at) VALUES ($1, $2, $3, $4)")
        .bind(issuer)
        .bind(subject)
        .bind(user_id)
        .bind(now_ts())
        .execute(&state.db)
        .await
        .map(|_| ())
        .map_err(|e| {
            // Two logins racing to link the same identity: the loser fails
            // and can simply retry.
            tracing::warn!("OIDC: linking failed: {e}");
            Failure::Failed
        })
}

/// Base username for a new account: preferred_username, else name, else the
/// email's local part. The caller appends -2, -3, ... to make it unique.
fn username_base(claims: &Claims) -> String {
    let candidates = [
        claims.preferred_username.as_deref(),
        claims.name.as_deref(),
        claims.email.as_deref().and_then(|e| e.split('@').next()),
    ];
    let base = candidates
        .into_iter()
        .flatten()
        .map(|s| s.trim().chars().filter(|c| !c.is_control()).take(50).collect::<String>())
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "user".into());
    base.trim().to_string()
}

async fn create_user(state: &AppState, oidc: &Oidc, claims: &Claims) -> Result<i64, Failure> {
    let base = username_base(claims);
    let mut username = base.clone();
    for n in 2.. {
        let taken: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE LOWER(username) = LOWER($1)")
            .bind(&username)
            .fetch_one(&state.db)
            .await
            .map_err(internal)?;
        if taken == 0 {
            break;
        }
        if n > 100 {
            return Err(Failure::Internal);
        }
        username = format!("{base}-{n}");
    }
    // Store the email only when verified and unused; it allows setting a
    // password later via "forgot password".
    let email = match claims.email.as_deref().map(str::trim) {
        Some(e) if e.contains('@') && (oidc.config.trust_email || email_is_verified(claims)) => {
            let taken: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email IS NOT NULL AND LOWER(email) = LOWER($1)")
                .bind(e)
                .fetch_one(&state.db)
                .await
                .map_err(internal)?;
            (taken == 0).then(|| e.to_string())
        }
        _ => None,
    };
    // Random unknown password: the account logs in through the provider
    // (and uses app passwords for OPDS) until the owner sets one.
    let hash = crate::db::hash_password(&random_hex(32)).map_err(internal)?;
    let is_admin = oidc.config.admin_group.as_deref().is_some_and(|g| in_group(claims, g));
    let insert = match &email {
        Some(email) => sqlx::query_scalar("INSERT INTO users (username, password_hash, is_admin, email) VALUES ($1, $2, $3, $4) RETURNING id")
            .bind(&username)
            .bind(&hash)
            .bind(DbFlag::from(is_admin))
            .bind(email),
        None => sqlx::query_scalar("INSERT INTO users (username, password_hash, is_admin) VALUES ($1, $2, $3) RETURNING id")
            .bind(&username)
            .bind(&hash)
            .bind(DbFlag::from(is_admin)),
    };
    let id: i64 = insert.fetch_one(&state.db).await.map_err(internal)?;
    link_identity(state, claims.iss.trim_end_matches('/'), &claims.sub, id).await?;
    crate::audit::log(state, Some((id, &username)), "user.oidc_created", json!({ "provider": oidc.config.name })).await;
    Ok(id)
}

fn in_group(claims: &Claims, group: &str) -> bool {
    match &claims.groups {
        Some(Value::Array(groups)) => groups.iter().any(|g| g.as_str() == Some(group)),
        Some(Value::String(g)) => g == group,
        _ => false,
    }
}

async fn session_user(state: &AppState, token: &str) -> Option<(i64, String)> {
    sqlx::query_as(
        "SELECT u.id, u.username FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.expires_at > $2",
    )
    .bind(token)
    .bind(now_ts())
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
}

async fn new_session(state: &AppState, user_id: i64) -> Result<String, Failure> {
    let token = auth::new_token();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&token)
        .bind(user_id)
        .bind(crate::db::ts_in_days(auth::SESSION_DAYS))
        .execute(&state.db)
        .await
        .map_err(internal)?;
    Ok(token)
}

#[derive(Debug, PartialEq)]
pub(crate) enum TokenError {
    Malformed,
    UnsupportedAlg,
    UnknownKey,
    BadSignature,
    WrongIssuer,
    WrongAudience,
    Expired,
    WrongNonce,
}

/// Checks an ID token: RS256 signature by a key in the JWKS, issuer,
/// audience, expiry (a minute of clock skew) and nonce.
pub(crate) fn verify_id_token(
    token: &str,
    jwks: &Jwks,
    issuer: &str,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<Claims, TokenError> {
    let mut parts = token.split('.');
    let (Some(h), Some(p), Some(s), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(TokenError::Malformed);
    };
    let decode = |part: &str| URL_SAFE_NO_PAD.decode(part.trim_end_matches('=')).map_err(|_| TokenError::Malformed);
    let header: Value = serde_json::from_slice(&decode(h)?).map_err(|_| TokenError::Malformed)?;
    if header.get("alg").and_then(Value::as_str) != Some("RS256") {
        return Err(TokenError::UnsupportedAlg);
    }
    let kid = header.get("kid").and_then(Value::as_str);
    let rsa_keys: Vec<&Jwk> = jwks.keys.iter().filter(|k| k.kty == "RSA" && k.use_.as_deref() != Some("enc")).collect();
    let jwk = match kid {
        Some(kid) => rsa_keys.iter().find(|k| k.kid.as_deref() == Some(kid)),
        None if rsa_keys.len() == 1 => rsa_keys.first(),
        None => None,
    }
    .ok_or(TokenError::UnknownKey)?;
    let (Some(n), Some(e)) = (&jwk.n, &jwk.e) else {
        return Err(TokenError::UnknownKey);
    };
    let key = RsaPublicKey::new(BigUint::from_bytes_be(&decode(n)?), BigUint::from_bytes_be(&decode(e)?))
        .map_err(|_| TokenError::UnknownKey)?;
    let signature = Signature::try_from(decode(s)?.as_slice()).map_err(|_| TokenError::Malformed)?;
    VerifyingKey::<Sha256>::new(key)
        .verify(format!("{h}.{p}").as_bytes(), &signature)
        .map_err(|_| TokenError::BadSignature)?;

    let claims: Claims = serde_json::from_slice(&decode(p)?).map_err(|_| TokenError::Malformed)?;
    if claims.iss.trim_end_matches('/') != issuer.trim_end_matches('/') {
        return Err(TokenError::WrongIssuer);
    }
    let audience_ok = match &claims.aud {
        Value::String(a) => a == client_id,
        Value::Array(a) => a.iter().any(|v| v.as_str() == Some(client_id)),
        _ => false,
    };
    if !audience_ok {
        return Err(TokenError::WrongAudience);
    }
    if claims.exp + 60 < now {
        return Err(TokenError::Expired);
    }
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(TokenError::WrongNonce);
    }
    if claims.sub.is_empty() {
        return Err(TokenError::Malformed);
    }
    Ok(claims)
}

#[derive(Serialize)]
pub struct AccountOidc {
    name: String,
    linked: bool,
}

/// GET /api/account/oidc: whether this account is linked (404 when OIDC is off).
pub async fn account_status(State(state): State<AppState>, user: AuthUser) -> Response {
    let Some(oidc) = &state.oidc else {
        return not_configured();
    };
    let linked: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM oidc_identities WHERE user_id = $1")
        .bind(user.0.id)
        .fetch_one(&state.db)
        .await
    {
        Ok(n) => n,
        Err(e) => {
            internal(e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response();
        }
    };
    Json(AccountOidc { name: oidc.config.name.clone(), linked: linked > 0 }).into_response()
}

#[cfg(test)]
pub(crate) mod test_support {
    //! An RSA key and a token signer for the OIDC tests.
    use super::*;
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::{SignatureEncoding, Signer};
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;

    pub struct TestKey {
        key: RsaPrivateKey,
    }

    impl TestKey {
        pub fn new() -> TestKey {
            TestKey { key: RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap() }
        }

        pub fn jwks_json(&self, kid: &str) -> Value {
            let public = self.key.to_public_key();
            json!({ "keys": [{
                "kty": "RSA", "kid": kid, "use": "sig", "alg": "RS256",
                "n": URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
                "e": URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
            }]})
        }

        pub fn jwks(&self, kid: &str) -> Jwks {
            serde_json::from_value(self.jwks_json(kid)).unwrap()
        }

        pub fn sign(&self, kid: &str, claims: &Value) -> String {
            let h = URL_SAFE_NO_PAD.encode(json!({ "alg": "RS256", "typ": "JWT", "kid": kid }).to_string());
            let p = URL_SAFE_NO_PAD.encode(claims.to_string());
            let sig = SigningKey::<Sha256>::new(self.key.clone()).sign(format!("{h}.{p}").as_bytes());
            format!("{h}.{p}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::TestKey;
    use super::*;

    const ISS: &str = "https://id.example.org";

    fn claims(now: i64) -> Value {
        json!({ "iss": ISS, "sub": "abc", "aud": "legejo", "exp": now + 300, "iat": now, "nonce": "n1" })
    }

    #[test]
    fn id_tokens_are_checked_on_every_point() {
        let key = TestKey::new();
        let jwks = key.jwks("k1");
        let now = 1_800_000_000;
        let ok = key.sign("k1", &claims(now));
        let c = verify_id_token(&ok, &jwks, ISS, "legejo", "n1", now).unwrap();
        assert_eq!(c.sub, "abc");
        // A trailing slash on the issuer is ignored.
        assert!(verify_id_token(&ok, &jwks, "https://id.example.org/", "legejo", "n1", now).is_ok());

        assert_eq!(verify_id_token(&ok, &jwks, ISS, "legejo", "n2", now).unwrap_err(), TokenError::WrongNonce);
        assert_eq!(verify_id_token(&ok, &jwks, ISS, "other", "n1", now).unwrap_err(), TokenError::WrongAudience);
        assert_eq!(verify_id_token(&ok, &jwks, "https://evil.example", "legejo", "n1", now).unwrap_err(), TokenError::WrongIssuer);
        assert_eq!(verify_id_token(&ok, &jwks, ISS, "legejo", "n1", now + 400).unwrap_err(), TokenError::Expired);
        assert_eq!(verify_id_token(&ok, &key.jwks("k2"), ISS, "legejo", "n1", now).unwrap_err(), TokenError::UnknownKey);

        // Another key's signature, a tampered payload, alg none.
        let other = TestKey::new();
        let forged = other.sign("k1", &claims(now));
        assert_eq!(verify_id_token(&forged, &jwks, ISS, "legejo", "n1", now).unwrap_err(), TokenError::BadSignature);
        let mut parts: Vec<String> = ok.split('.').map(Into::into).collect();
        let mut tampered = claims(now);
        tampered["sub"] = json!("admin");
        parts[1] = URL_SAFE_NO_PAD.encode(tampered.to_string());
        assert_eq!(verify_id_token(&parts.join("."), &jwks, ISS, "legejo", "n1", now).unwrap_err(), TokenError::BadSignature);
        parts[0] = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        assert_eq!(verify_id_token(&parts.join("."), &jwks, ISS, "legejo", "n1", now).unwrap_err(), TokenError::UnsupportedAlg);

        // aud as an array.
        let mut multi = claims(now);
        multi["aud"] = json!(["x", "legejo"]);
        assert!(verify_id_token(&key.sign("k1", &multi), &jwks, ISS, "legejo", "n1", now).is_ok());
    }

    #[test]
    fn usernames_and_groups() {
        let mut c = Claims { email: Some("carol@example.org".into()), ..Default::default() };
        assert_eq!(username_base(&c), "carol");
        c.name = Some("Carol von Essen".into());
        assert_eq!(username_base(&c), "Carol von Essen");
        c.preferred_username = Some("  ".into());
        assert_eq!(username_base(&c), "Carol von Essen");
        c.groups = Some(json!(["readers", "legejo-admins"]));
        assert!(in_group(&c, "legejo-admins"));
        assert!(!in_group(&c, "admins"));
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let f = Flow { state: "s".into(), verifier: "v".into(), nonce: "n".into(), link: true }.encode();
        let back = Flow::decode(&f).unwrap();
        assert!(back.link && back.state == "s" && back.verifier == "v" && back.nonce == "n");
        assert!(Flow::decode("a.b").is_none());
    }
}
