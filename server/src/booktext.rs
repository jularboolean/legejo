//! Plain text out of an EPUB: its sections in reading order, with titles from
//! the table of contents, for the tools that let an assistant read and search
//! a book (mcp.rs).

use epub::doc::{EpubDoc, NavPoint};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// One spine item with text, in reading order.
pub struct Section {
    /// The title from the table of contents, when an entry points here.
    pub title: Option<String>,
    pub text: String,
    /// Where the section starts and ends in the book, 0–1 by text length.
    /// Reading positions from the reader, Kobo and KOReader are close to,
    /// but not exactly, this measure.
    pub start: f64,
    pub end: f64,
}

/// The whole book as text. Sections without text (covers, image pages) are
/// left out.
pub fn sections(bytes: Vec<u8>) -> anyhow::Result<Vec<Section>> {
    let mut doc = EpubDoc::from_reader(Cursor::new(bytes)).map_err(|e| anyhow::anyhow!("could not parse epub: {e}"))?;
    let mut titles: HashMap<PathBuf, String> = HashMap::new();
    collect_titles(&doc.toc, &mut titles);

    let spine: Vec<String> = doc.spine.iter().map(|item| item.idref.clone()).collect();
    let mut out: Vec<Section> = Vec::new();
    for id in spine {
        let Some(resource) = doc.resources.get(&id) else { continue };
        let path = resource.path.clone();
        let Some((html, _mime)) = doc.get_resource_str(&id) else { continue };
        let text = html_to_text(&html);
        if text.chars().count() < 2 {
            continue;
        }
        out.push(Section { title: titles.get(&path).cloned(), text, start: 0.0, end: 0.0 });
    }
    let total: usize = out.iter().map(|s| s.text.len()).sum();
    let mut seen = 0usize;
    for section in &mut out {
        section.start = if total == 0 { 0.0 } else { seen as f64 / total as f64 };
        seen += section.text.len();
        section.end = if total == 0 { 1.0 } else { seen as f64 / total as f64 };
    }
    Ok(out)
}

/// The first table-of-contents label for each document (fragments dropped).
fn collect_titles(points: &[NavPoint], into: &mut HashMap<PathBuf, String>) {
    for point in points {
        let label = point.label.split_whitespace().collect::<Vec<_>>().join(" ");
        let path = strip_fragment(&point.content);
        if !label.is_empty() {
            into.entry(path).or_insert(label);
        }
        collect_titles(&point.children, into);
    }
}

fn strip_fragment(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    PathBuf::from(s.split('#').next().unwrap_or(""))
}

const BLOCK_TAGS: [&str; 22] = [
    "p", "div", "br", "h1", "h2", "h3", "h4", "h5", "h6", "li", "tr", "blockquote", "section", "article", "hr", "pre",
    "table", "ul", "ol", "dt", "dd", "figcaption",
];
/// Elements whose content is not part of the text.
const SKIPPED: [&str; 4] = ["script", "style", "head", "svg"];

/// XHTML to readable text: block elements become line breaks, entities are
/// decoded, images are represented by their alt text.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let mut skipping: Option<&'static str> = None;
    while let Some(lt) = rest.find('<') {
        if skipping.is_none() {
            push_text(&mut out, &rest[..lt]);
        }
        rest = &rest[lt..];
        if rest.starts_with("<!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .rsplit(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if let Some(open) = skipping {
            if closing && name == open {
                skipping = None;
            }
            continue;
        }
        if let Some(skip) = SKIPPED.iter().find(|s| **s == name) {
            if !closing && !tag.ends_with('/') {
                skipping = Some(skip);
            }
            continue;
        }
        if name == "img" {
            if let Some(alt) = attribute(tag, "alt").filter(|a| !a.trim().is_empty()) {
                out.push_str(" [image: ");
                push_text(&mut out, &alt);
                out.push_str("] ");
            }
        } else if BLOCK_TAGS.contains(&name.as_str()) {
            out.push('\n');
        }
    }
    if skipping.is_none() {
        push_text(&mut out, rest);
    }
    tidy(&out)
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let at = from + found;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let after = tag[at + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let value = after[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let inner = &value[1..];
                return inner.find(quote).map(|end| inner[..end].to_string());
            }
            return Some(value.split_whitespace().next().unwrap_or("").to_string());
        }
        from = at + name.len();
    }
    None
}

/// Append text with character references decoded.
fn push_text(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let end = rest[1..].find(|c: char| c == ';' || c.is_whitespace() || c == '&').map(|i| i + 1);
        match end {
            Some(end) if rest.as_bytes()[end] == b';' => {
                match decode_entity(&rest[1..end]) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=end]),
                }
                rest = &rest[end + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
}

fn decode_entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "shy" => '\u{ad}',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "laquo" => '«',
        "raquo" => '»',
        "copy" => '©',
        _ => return None,
    })
}

/// One line per block: collapse runs of spaces and drop soft hyphens and
/// empty lines (markup between blocks leaves many).
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let line: String = line.replace('\u{ad}', "").split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

/// The largest index not above `at` that is a character boundary.
pub fn floor_boundary(text: &str, at: usize) -> usize {
    let mut i = at.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Case-insensitive matches of `needle`, as byte offsets into `text`.
pub fn find_all(text: &str, needle: &str, limit: usize) -> Vec<usize> {
    let hay = text.to_lowercase();
    let needle = needle.to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    if hay.len() != text.len() {
        // Lowercasing changed byte lengths (rare scripts): fall back to exact matching.
        return text.match_indices(needle.as_str()).map(|(i, _)| i).take(limit).collect();
    }
    hay.match_indices(needle.as_str()).map(|(i, _)| i).take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_readable_text() {
        let html = r#"<html><head><title>Ignored</title><style>p { color: red }</style></head>
            <body><h1>Chapter&nbsp;One</h1><p>It was a <em>dark</em> &amp; stormy night&#8212;really.</p>
            <!-- a comment --><p>Second<br/>line. &ldquo;Quoted&rdquo; &unknown; A &B.</p>
            <img src="x.png" alt="A map of the island"/><script>alert(1)</script><ul><li>one</li><li>two</li></ul></body></html>"#;
        let text = html_to_text(html);
        assert_eq!(
            text,
            "Chapter One\nIt was a dark & stormy night—really.\nSecond\nline. “Quoted” &unknown; A &B.\n[image: A map of the island]\none\ntwo"
        );
        assert!(!text.contains("Ignored") && !text.contains("alert") && !text.contains("color"));
    }

    #[test]
    fn search_and_boundaries() {
        let text = "Röda rummet. Han såg det RÖDA huset.";
        assert_eq!(find_all(text, "röda", 10).len(), 2);
        assert_eq!(find_all(text, "", 10).len(), 0);
        assert_eq!(find_all(text, "röda", 1).len(), 1);
        // 'ö' is two bytes: an index inside it moves back to its start.
        assert_eq!(floor_boundary(text, 2), 1);
        assert_eq!(floor_boundary(text, 500), text.len());
    }
}
