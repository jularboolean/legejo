//! Everything Legejo fetches from other servers goes through here.
//!
//! SSRF is the main risk federation introduces: every URL (actors, keys,
//! EPUBs, covers, nodeinfo) comes from outside, and the private network may
//! host unauthenticated services. The resolver therefore drops private,
//! loopback, link-local, CGNAT and ULA addresses after the DNS lookup (so
//! DNS rebinding cannot get around a name check), IP literals are checked
//! before the request, only https is allowed, every redirect hop is checked
//! again (at most three), and responses are cut at a size limit.

use crate::AppState;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::Url;
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

pub const JSON_LIMIT: usize = 1024 * 1024;

/// Addresses no federated fetch may reach.
pub fn forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_private()                       // 10/8, 172.16/12, 192.168/16
                || v4.is_loopback()               // 127/8
                || v4.is_link_local()             // 169.254/16
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || o[0] == 0                      // 0/8
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10: CGNAT, Tailscale
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 benchmarking
                || o[0] >= 240                    // reserved
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return forbidden(IpAddr::V4(v4));
            }
            let s = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00      // fc00::/7
                || (s[0] & 0xffc0) == 0xfe80      // fe80::/10
                || (s[0] == 0x64 && s[1] == 0xff9b) // NAT64 can reach v4 space
                || (s[0] == 0x2001 && s[1] == 0xdb8) // documentation
        }
    }
}

struct SafeResolver {
    allow_private: bool,
}

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_private = self.allow_private;
        Box::pin(async move {
            let host = name.as_str().to_string();
            let found: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let addrs: Vec<SocketAddr> = found.into_iter().filter(|a| allow_private || !forbidden(a.ip())).collect();
            if addrs.is_empty() {
                tracing::warn!("fed: SSRF blocked: {host} resolves to private addresses only");
                return Err(format!("SSRF blocked: {host} resolves to private addresses only").into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// Scheme and literal-IP checks; DNS names are checked by the resolver.
pub fn check_url(url: &Url, allow_private: bool) -> Result<(), String> {
    match url.scheme() {
        "https" => {}
        "http" if allow_private => {}
        s => return Err(format!("scheme {s} not allowed")),
    }
    let ip = match url.host() {
        None => return Err("no host".into()),
        Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            if !allow_private && (d == "localhost" || d.ends_with(".localhost") || d.ends_with(".internal") || d.ends_with(".consul") || !d.contains('.')) {
                return Err(format!("SSRF blocked: {d}"));
            }
            None
        }
    };
    if let Some(ip) = ip {
        if !allow_private && forbidden(ip) {
            return Err(format!("SSRF blocked: {ip}"));
        }
    }
    Ok(())
}

pub fn client(agent: &str, allow_private: bool) -> anyhow::Result<reqwest::Client> {
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 3 {
            attempt.error("too many redirects")
        } else if let Err(e) = check_url(attempt.url(), allow_private) {
            tracing::warn!("fed: SSRF blocked redirect: {e}");
            attempt.error(e)
        } else {
            attempt.follow()
        }
    });
    Ok(reqwest::Client::builder()
        .user_agent(agent)
        .dns_resolver(Arc::new(SafeResolver { allow_private }))
        .redirect(policy)
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()?)
}

#[derive(Debug)]
pub enum FetchError {
    Blocked(String),
    Status(u16),
    Network(String),
    TooLarge,
    BadJson,
    Off,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Blocked(e) => write!(f, "blocked ({e})"),
            FetchError::Status(s) => write!(f, "HTTP {s}"),
            FetchError::Network(e) => write!(f, "network error ({e})"),
            FetchError::TooLarge => write!(f, "response too large"),
            FetchError::BadJson => write!(f, "not JSON"),
            FetchError::Off => write!(f, "federation is off"),
        }
    }
}

fn parse(url: &str, allow_private: bool) -> Result<Url, FetchError> {
    let url = Url::parse(url).map_err(|_| FetchError::Blocked("not a URL".into()))?;
    check_url(&url, allow_private).map_err(|e| {
        tracing::warn!("fed: SSRF blocked: {e}");
        FetchError::Blocked(e)
    })?;
    Ok(url)
}

async fn read_limited(mut resp: reqwest::Response, limit: usize) -> Result<Vec<u8>, FetchError> {
    if resp.content_length().is_some_and(|l| l as usize > limit) {
        return Err(FetchError::TooLarge);
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| FetchError::Network(e.to_string()))? {
        if out.len() + chunk.len() > limit {
            return Err(FetchError::TooLarge);
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// GET, signed by the instance actor (servers with authorized fetch demand it).
async fn signed_get(state: &AppState, url: &str, accept: &str) -> Result<reqwest::Response, FetchError> {
    let config = super::active(state).await.ok_or(FetchError::Off)?;
    let url = parse(url, config.allow_private)?;
    let actor = config.instance_actor();
    let (key, _) = super::sig::key_for(state, &actor).await.map_err(|e| FetchError::Network(e.to_string()))?;
    let mut req = state.fed.http.get(url.clone()).header("accept", accept);
    for (name, value) in super::sig::sign("GET", &url, None, &format!("{actor}#main-key"), &key) {
        req = req.header(name, value);
    }
    let resp = req.send().await.map_err(send_error)?;
    if !resp.status().is_success() {
        return Err(FetchError::Status(resp.status().as_u16()));
    }
    Ok(resp)
}

/// The resolver and the redirect policy report a blocked address as a
/// request error; tell it apart from a network failure.
fn send_error(e: reqwest::Error) -> FetchError {
    if format!("{e:?}").contains("SSRF") {
        FetchError::Blocked(e.to_string())
    } else {
        FetchError::Network(e.to_string())
    }
}

/// What an unsigned fetch brought back.
pub struct Fetched {
    pub bytes: Vec<u8>,
    /// Where the response came from, after redirects: relative links in it
    /// are resolved against this.
    pub url: Url,
    pub content_type: Option<String>,
}

/// A public document fetched without a signature (an OPDS catalog, a book
/// or a cover in one). The address checks are the same as for every other
/// fetch, and federation does not have to be on.
pub async fn get_public(
    state: &AppState,
    url: &str,
    accept: &str,
    limit: usize,
    timeout: std::time::Duration,
) -> Result<Fetched, FetchError> {
    let allow_private = state.fed.config.as_ref().is_some_and(|c| c.allow_private);
    let url = parse(url, allow_private)?;
    let resp = state.fed.http.get(url).header("accept", accept).timeout(timeout).send().await.map_err(send_error)?;
    if !resp.status().is_success() {
        return Err(FetchError::Status(resp.status().as_u16()));
    }
    let url = resp.url().clone();
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).map(str::to_string);
    let bytes = read_limited(resp, limit).await?;
    Ok(Fetched { bytes, url, content_type })
}

/// An ActivityPub document (actor, object, collection).
pub async fn get_json(state: &AppState, url: &str) -> Result<Value, FetchError> {
    let resp = signed_get(
        state,
        url,
        "application/activity+json, application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
    )
    .await?;
    let bytes = read_limited(resp, JSON_LIMIT).await?;
    serde_json::from_slice(&bytes).map_err(|_| FetchError::BadJson)
}

/// Plain JSON (WebFinger, nodeinfo).
pub async fn get_plain_json(state: &AppState, url: &str) -> Result<Value, FetchError> {
    let resp = signed_get(state, url, "application/jrd+json, application/json").await?;
    let bytes = read_limited(resp, JSON_LIMIT).await?;
    serde_json::from_slice(&bytes).map_err(|_| FetchError::BadJson)
}

/// A file (EPUB), at most `limit` bytes.
pub async fn get_bytes(state: &AppState, url: &str, limit: usize) -> Result<Vec<u8>, FetchError> {
    let resp = signed_get(state, url, "application/epub+zip, */*").await?;
    read_limited(resp, limit).await
}

/// POST an activity to an inbox, signed as `actor_iri`. Returns the status.
pub async fn post_activity(state: &AppState, inbox: &str, actor_iri: &str, body: &str) -> Result<u16, FetchError> {
    let config = super::active(state).await.ok_or(FetchError::Off)?;
    let url = parse(inbox, config.allow_private)?;
    let (key, _) = super::sig::key_for(state, actor_iri).await.map_err(|e| FetchError::Network(e.to_string()))?;
    let mut req = state
        .fed
        .http
        .post(url.clone())
        .header("content-type", super::AP_JSON)
        .header("accept", super::AP_JSON)
        .body(body.to_string());
    for (name, value) in super::sig::sign("POST", &url, Some(body.as_bytes()), &format!("{actor_iri}#main-key"), &key) {
        req = req.header(name, value);
    }
    let resp = req.send().await.map_err(|e| FetchError::Network(e.to_string()))?;
    Ok(resp.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_ranges_are_forbidden() {
        for ip in [
            "10.0.1.10", "127.0.0.1", "169.254.169.254", "172.16.0.1", "172.31.255.255", "192.168.1.1",
            "100.64.0.1", "100.100.100.100", "0.0.0.0", "::1", "fc00::1", "fd12::1", "fe80::1",
            "::ffff:10.0.1.10", "64:ff9b::a00:10a",
        ] {
            assert!(forbidden(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "104.21.3.4", "172.32.0.1", "100.128.0.1", "2606:4700::1"] {
            assert!(!forbidden(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn urls_are_checked_before_any_request() {
        let ok = |u: &str| check_url(&Url::parse(u).unwrap(), false);
        assert!(ok("https://b.example/ap/inbox").is_ok());
        assert!(ok("http://b.example/ap/inbox").is_err());
        assert!(ok("https://10.0.1.10:6379/").is_err());
        assert!(ok("https://[::1]/").is_err());
        assert!(ok("https://localhost/").is_err());
        assert!(ok("https://redis.service.consul/").is_err());
        assert!(ok("https://intranet/").is_err());
        assert!(ok("file:///etc/passwd").is_err());
        assert!(check_url(&Url::parse("http://127.0.0.1:3998/").unwrap(), true).is_ok());
    }

    #[tokio::test]
    async fn resolver_drops_private_answers() {
        let r = SafeResolver { allow_private: false };
        let res = r.resolve("localhost".parse().unwrap()).await;
        assert!(res.is_err());
    }
}
