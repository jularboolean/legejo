//! What changed in each version, for the app to show after an upgrade. The
//! changelog is built into the binary, so the answer needs no network.

use crate::auth::AuthUser;
use axum::extract::Query;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");
const MOST: usize = 20;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Section {
    pub version: String,
    pub date: String,
    /// The section's text, Markdown, without its heading.
    pub body: String,
}

/// The sections, newest first, as the `## X.Y.Z (date)` headings delimit them.
pub fn sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut body: Vec<&str> = Vec::new();
    let mut current: Option<(String, String)> = None;
    let close = |current: &mut Option<(String, String)>, body: &mut Vec<&str>, out: &mut Vec<Section>| {
        if let Some((version, date)) = current.take() {
            out.push(Section { version, date, body: body.join("\n").trim().to_string() });
        }
        body.clear();
    };
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            close(&mut current, &mut body, &mut out);
            let (version, date) = match heading.split_once(" (") {
                Some((v, d)) => (v.trim(), d.trim_end_matches(')').trim()),
                None => (heading.trim(), ""),
            };
            if parse_version(version).is_some() {
                current = Some((version.to_string(), date.to_string()));
            }
        } else if current.is_some() {
            body.push(line);
        }
    }
    close(&mut current, &mut body, &mut out);
    out
}

/// "1.17.1" as something that compares.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

#[derive(Deserialize)]
pub struct Since {
    since: Option<String>,
}

/// GET /api/changelog?since=X.Y.Z: what changed after `since`, up to the
/// version running, newest first. Without `since`, the running version's own.
pub async fn get(_user: AuthUser, Query(q): Query<Since>) -> Json<Value> {
    let current = env!("CARGO_PKG_VERSION");
    Json(json!({ "version": current, "sections": since(CHANGELOG, current, q.since.as_deref()) }))
}

pub fn since(text: &str, current: &str, since: Option<&str>) -> Vec<Section> {
    let now = parse_version(current);
    let from = since.and_then(parse_version);
    sections(text)
        .into_iter()
        .filter(|s| {
            let Some(v) = parse_version(&s.version) else { return false };
            if now.is_some_and(|c| v > c) {
                return false;
            }
            match from {
                Some(f) => v > f,
                None => Some(v) == now,
            }
        })
        .take(MOST)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Changelog\n\nIntro.\n\n## 1.2.0 (2026-10-12)\n\n- Two.\n- Things.\n\n## 1.1.1 (2026-10-11)\n\n- Patch.\n\n## 1.1.0 (2026-10-10)\n\n- One.\n\n## Unreleased\n\n- Not a version.\n";

    #[test]
    fn sections_come_out_newest_first_with_their_text() {
        let s = sections(SAMPLE);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].version.as_str(), s[0].date.as_str()), ("1.2.0", "2026-10-12"));
        assert_eq!(s[0].body, "- Two.\n- Things.");
        assert_eq!(s[2].version, "1.1.0");
    }

    #[test]
    fn what_changed_since_a_version_stops_at_the_one_running() {
        let v = |list: Vec<Section>| list.into_iter().map(|s| s.version).collect::<Vec<_>>();
        assert_eq!(v(since(SAMPLE, "1.2.0", Some("1.1.0"))), ["1.2.0", "1.1.1"]);
        assert_eq!(v(since(SAMPLE, "1.1.1", Some("1.1.0"))), ["1.1.1"], "a newer section in the file than the binary is not shown");
        assert_eq!(v(since(SAMPLE, "1.2.0", Some("1.2.0"))), Vec::<String>::new());
        assert_eq!(v(since(SAMPLE, "1.2.0", None)), ["1.2.0"]);
        assert_eq!(v(since(SAMPLE, "1.2.0", Some("nonsense"))), ["1.2.0"]);
    }

    #[test]
    fn the_real_changelog_has_the_running_version_on_top() {
        let s = sections(CHANGELOG);
        assert_eq!(s.first().map(|s| s.version.as_str()), Some(env!("CARGO_PKG_VERSION")), "CHANGELOG.md must begin with the version being built");
    }
}
