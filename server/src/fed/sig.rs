//! RSA keys and HTTP Signatures (draft-cavage-http-signatures, rsa-sha256):
//! the dialect Mastodon, BookWyrm and relays speak.
//!
//! Outgoing: `(request-target) host date [digest]`, signed with the key of
//! the actor the request speaks for. Incoming: the Digest must match the
//! body, Date must be within five minutes, and the signature must verify
//! against the key the keyId points at.

use crate::AppState;
use base64::Engine;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// The actor's key pair, created on first use (RSA 2048, stored in ap_keys
/// so database backups include it). Returns (private key, public PEM).
pub async fn key_for(state: &AppState, actor_iri: &str) -> anyhow::Result<(RsaPrivateKey, String)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT private_pem, public_pem FROM ap_keys WHERE actor_iri = $1")
            .bind(actor_iri)
            .fetch_optional(&state.db)
            .await?;
    if let Some((private_pem, public_pem)) = row {
        return Ok((RsaPrivateKey::from_pkcs8_pem(&private_pem)?, public_pem));
    }
    let key = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut rand::thread_rng(), 2048)).await??;
    let private_pem = key.to_pkcs8_pem(LineEnding::LF)?.to_string();
    let public_pem = key.to_public_key().to_public_key_pem(LineEnding::LF)?;
    // Two first requests may race; whoever loses reads the winner's key.
    sqlx::query("INSERT INTO ap_keys (actor_iri, public_pem, private_pem) VALUES ($1, $2, $3) ON CONFLICT (actor_iri) DO NOTHING")
        .bind(actor_iri)
        .bind(&public_pem)
        .bind(&private_pem)
        .execute(&state.db)
        .await?;
    let (private_pem, public_pem): (String, String) =
        sqlx::query_as("SELECT private_pem, public_pem FROM ap_keys WHERE actor_iri = $1")
            .bind(actor_iri)
            .fetch_one(&state.db)
            .await?;
    Ok((RsaPrivateKey::from_pkcs8_pem(&private_pem)?, public_pem))
}

pub fn parse_public_pem(pem: &str) -> Option<RsaPublicKey> {
    RsaPublicKey::from_public_key_pem(pem.trim())
        .ok()
        .or_else(|| rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_pem(pem.trim()).ok())
}

pub fn digest_header(body: &[u8]) -> String {
    format!("SHA-256={}", B64.encode(Sha256::digest(body)))
}

/// The headers to add to an outgoing request: Date, (Digest), Signature.
/// `host` is the URL's host as sent in the Host header.
pub fn sign(
    method: &str,
    url: &reqwest::Url,
    body: Option<&[u8]>,
    key_id: &str,
    key: &RsaPrivateKey,
) -> Vec<(&'static str, String)> {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let host = match url.port() {
        Some(p) => format!("{}:{p}", url.host_str().unwrap_or("")),
        None => url.host_str().unwrap_or("").to_string(),
    };
    let target = match url.query() {
        Some(q) => format!("{} {}?{q}", method.to_lowercase(), url.path()),
        None => format!("{} {}", method.to_lowercase(), url.path()),
    };
    let mut out = vec![("date", date.clone())];
    let (names, signing_string) = match body {
        Some(body) => {
            let digest = digest_header(body);
            let s = format!("(request-target): {target}\nhost: {host}\ndate: {date}\ndigest: {digest}");
            out.push(("digest", digest));
            ("(request-target) host date digest", s)
        }
        None => ("(request-target) host date", format!("(request-target): {target}\nhost: {host}\ndate: {date}")),
    };
    let signature = SigningKey::<Sha256>::new(key.clone()).sign(signing_string.as_bytes());
    out.push((
        "signature",
        format!(
            "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"{names}\",signature=\"{}\"",
            B64.encode(signature.to_bytes())
        ),
    ));
    out
}

#[derive(Debug, Clone)]
pub struct ParsedSignature {
    pub key_id: String,
    pub headers: Vec<String>,
    pub signature: Vec<u8>,
}

/// `keyId="…",algorithm="…",headers="…",signature="…"`
pub fn parse_signature(value: &str) -> Option<ParsedSignature> {
    let mut key_id = None;
    let mut headers = None;
    let mut signature = None;
    let mut rest = value.trim();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let name = rest[..eq].trim().trim_start_matches(',').trim().to_ascii_lowercase();
        let after = &rest[eq + 1..];
        let (val, next) = if let Some(stripped) = after.strip_prefix('"') {
            let end = stripped.find('"')?;
            (&stripped[..end], &stripped[end + 1..])
        } else {
            let end = after.find(',').unwrap_or(after.len());
            (&after[..end], &after[end..])
        };
        match name.as_str() {
            "keyid" => key_id = Some(val.to_string()),
            "headers" => headers = Some(val.split_whitespace().map(|h| h.to_ascii_lowercase()).collect()),
            "signature" => signature = B64.decode(val).ok(),
            _ => {}
        }
        rest = next.trim_start_matches(',').trim();
    }
    Some(ParsedSignature {
        key_id: key_id?,
        // The spec's default when headers is absent is just Date.
        headers: headers.unwrap_or_else(|| vec!["date".into()]),
        signature: signature?,
    })
}

/// Why an incoming signature is refused.
#[derive(Debug, PartialEq, Eq)]
pub enum SigError {
    Missing,
    Malformed,
    /// The signed headers must cover the request line, the host and the date
    /// (and the digest for a body).
    Coverage,
    DigestMismatch,
    DateSkew,
    BadSignature,
}

impl SigError {
    pub fn as_str(&self) -> &'static str {
        match self {
            SigError::Missing => "unsigned",
            SigError::Malformed => "malformed signature",
            SigError::Coverage => "signature does not cover request-target/host/date/digest",
            SigError::DigestMismatch => "Digest does not match body",
            SigError::DateSkew => "Date outside +/-5 min",
            SigError::BadSignature => "bad signature",
        }
    }
}

/// Checks that need no key: coverage, Digest and Date.
pub fn precheck(
    sig: &ParsedSignature,
    headers: &axum::http::HeaderMap,
    body: Option<&[u8]>,
) -> Result<(), SigError> {
    let has = |h: &str| sig.headers.iter().any(|x| x == h);
    if !has("(request-target)") || !has("host") || !(has("date") || has("(created)")) {
        return Err(SigError::Coverage);
    }
    if let Some(body) = body {
        if !has("digest") {
            return Err(SigError::Coverage);
        }
        let sent = headers.get("digest").and_then(|v| v.to_str().ok()).ok_or(SigError::DigestMismatch)?;
        // "SHA-256=…", possibly among other algorithms.
        let ours = digest_header(body);
        if !sent.split(',').any(|d| d.trim().eq_ignore_ascii_case(&ours) || d.trim() == ours) {
            return Err(SigError::DigestMismatch);
        }
    }
    if has("date") {
        let date = headers.get("date").and_then(|v| v.to_str().ok()).ok_or(SigError::DateSkew)?;
        let when = httpdate::parse_http_date(date).map_err(|_| SigError::DateSkew)?;
        let now = std::time::SystemTime::now();
        let skew = now.duration_since(when).or_else(|_| when.duration_since(now)).map_err(|_| SigError::DateSkew)?;
        if skew > std::time::Duration::from_secs(300) {
            return Err(SigError::DateSkew);
        }
    }
    Ok(())
}

/// Verify the signature itself against the actor's public key.
pub fn verify(
    sig: &ParsedSignature,
    method: &str,
    path_and_query: &str,
    headers: &axum::http::HeaderMap,
    public_pem: &str,
) -> Result<(), SigError> {
    let mut lines = Vec::with_capacity(sig.headers.len());
    for name in &sig.headers {
        let value = if name == "(request-target)" {
            format!("{} {path_and_query}", method.to_lowercase())
        } else {
            let values: Vec<&str> = headers.get_all(name.as_str()).iter().filter_map(|v| v.to_str().ok()).collect();
            if values.is_empty() {
                return Err(SigError::Malformed);
            }
            values.join(", ")
        };
        lines.push(format!("{name}: {value}"));
    }
    let key = parse_public_pem(public_pem).ok_or(SigError::Malformed)?;
    let signature = Signature::try_from(sig.signature.as_slice()).map_err(|_| SigError::Malformed)?;
    VerifyingKey::<Sha256>::new(key)
        .verify(lines.join("\n").as_bytes(), &signature)
        .map_err(|_| SigError::BadSignature)
}

/// keyId without its fragment: the actor document holding the key.
pub fn key_owner(key_id: &str) -> &str {
    key_id.split('#').next().unwrap_or(key_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    fn key() -> RsaPrivateKey {
        RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap()
    }

    fn headers_of(signed: &[(&'static str, String)], host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("host", HeaderValue::from_str(host).unwrap());
        for (n, v) in signed {
            h.insert(HeaderName::from_static(n), HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn round_trip_and_tampering() {
        let k = key();
        let pem = k.to_public_key().to_public_key_pem(LineEnding::LF).unwrap();
        let url = reqwest::Url::parse("https://b.example/ap/inbox").unwrap();
        let body = br#"{"type":"Follow"}"#;
        let signed = sign("POST", &url, Some(body), "https://a.example/ap/actor#main-key", &k);
        let headers = headers_of(&signed, "b.example");
        let sig = parse_signature(headers.get("signature").unwrap().to_str().unwrap()).unwrap();
        assert_eq!(key_owner(&sig.key_id), "https://a.example/ap/actor");

        assert_eq!(precheck(&sig, &headers, Some(body)), Ok(()));
        assert_eq!(verify(&sig, "POST", "/ap/inbox", &headers, &pem), Ok(()));

        // A changed body no longer matches the Digest,
        assert_eq!(precheck(&sig, &headers, Some(b"{\"type\":\"Delete\"}")), Err(SigError::DigestMismatch));
        // another path or host breaks the signature,
        assert_eq!(verify(&sig, "POST", "/ap/shelves/x/inbox", &headers, &pem), Err(SigError::BadSignature));
        let other = headers_of(&signed, "c.example");
        assert_eq!(verify(&sig, "POST", "/ap/inbox", &other, &pem), Err(SigError::BadSignature));
        // and another key does not verify.
        let pem2 = key().to_public_key().to_public_key_pem(LineEnding::LF).unwrap();
        assert_eq!(verify(&sig, "POST", "/ap/inbox", &headers, &pem2), Err(SigError::BadSignature));
    }

    #[test]
    fn stale_dates_and_thin_coverage_are_refused() {
        let k = key();
        let url = reqwest::Url::parse("https://b.example/ap/inbox").unwrap();
        let body = b"{}";
        let signed = sign("POST", &url, Some(body), "k", &k);
        let mut headers = headers_of(&signed, "b.example");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        headers.insert("date", HeaderValue::from_str(&httpdate::fmt_http_date(old)).unwrap());
        let sig = parse_signature(headers.get("signature").unwrap().to_str().unwrap()).unwrap();
        assert_eq!(precheck(&sig, &headers, Some(body)), Err(SigError::DateSkew));

        let thin = parse_signature(r#"keyId="k",headers="date",signature="AAAA""#).unwrap();
        assert_eq!(precheck(&thin, &headers, Some(body)), Err(SigError::Coverage));
    }

    #[test]
    fn parses_mastodon_style_headers() {
        let s = parse_signature(
            r#"keyId="https://m.example/users/a#main-key",algorithm="rsa-sha256",headers="(request-target) host date digest content-type",signature="AQID""#,
        )
        .unwrap();
        assert_eq!(s.key_id, "https://m.example/users/a#main-key");
        assert_eq!(s.headers.len(), 5);
        assert_eq!(s.signature, vec![1, 2, 3]);
        assert!(parse_signature("garbage").is_none());
    }
}
