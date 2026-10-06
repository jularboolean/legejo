//! Looking after the EPUB file itself: a health check, repairs that are safe
//! to make without asking, and writing the catalog's metadata and cover back
//! into the file.
//!
//! Everything here works on bytes and returns bytes. The package document is
//! edited as text, so whatever is not touched stays byte-identical, and the
//! other entries of the archive are copied as they are.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};

/// One thing wrong with a file.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Issue {
    pub code: String,
    /// `repair` can take care of it.
    pub fixable: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn issue(code: &str, fixable: bool) -> Issue {
    Issue { code: code.to_string(), fixable, count: 0, examples: Vec::new() }
}

/// Raised when the check learns something new, so that stored results from
/// an older check are made again.
pub const CHECK_VERSION: u32 = 1;

/// The result of the health check, as stored with a book.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Health {
    /// Which version of the check produced this (see CHECK_VERSION).
    #[serde(default)]
    pub v: u32,
    /// What is still wrong.
    pub issues: Vec<Issue>,
    /// What has been repaired in this file.
    #[serde(default)]
    pub fixed: Vec<String>,
}

/// What the catalog knows and the file should say.
pub struct Meta<'a> {
    pub title: &'a str,
    pub author: Option<&'a str>,
    pub language: Option<&'a str>,
    pub series: Option<&'a str>,
    pub series_index: Option<f64>,
}

const MIMETYPE: &str = "application/epub+zip";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const EXAMPLES: usize = 5;

// ---- A small tag scanner ---------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Open,
    Close,
    Empty,
}

/// One tag in a document: `text[start..end]` is `<…>`.
#[derive(Clone, Debug)]
struct Tag {
    start: usize,
    end: usize,
    kind: Kind,
    /// The name as written, prefix included.
    name: String,
}

impl Tag {
    fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or("")
    }
    fn prefix(&self) -> &str {
        match self.name.rfind(':') {
            Some(i) => &self.name[..=i],
            None => "",
        }
    }
    fn raw<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

/// Every element tag, in order. Comments, CDATA, processing instructions and
/// declarations are skipped.
fn tags(text: &str) -> Vec<Tag> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(lt) = text[i..].find('<').map(|p| p + i) {
        let rest = &text[lt..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(text.len(), |e| lt + e + 3);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i = rest.find("]]>").map_or(text.len(), |e| lt + e + 3);
            continue;
        }
        if rest.starts_with("<?") {
            i = rest.find("?>").map_or(text.len(), |e| lt + e + 2);
            continue;
        }
        // The closing '>' is the first one outside a quoted attribute value.
        let mut quote = 0u8;
        let mut gt = None;
        for (j, b) in bytes.iter().enumerate().skip(lt + 1) {
            if quote != 0 {
                if *b == quote {
                    quote = 0;
                }
            } else if *b == b'"' || *b == b'\'' {
                quote = *b;
            } else if *b == b'>' {
                gt = Some(j);
                break;
            }
        }
        let Some(gt) = gt else { break };
        i = gt + 1;
        if rest.starts_with("<!") {
            continue;
        }
        let inner = &text[lt + 1..gt];
        let (kind, body) = if let Some(b) = inner.strip_prefix('/') {
            (Kind::Close, b)
        } else if let Some(b) = inner.strip_suffix('/') {
            (Kind::Empty, b)
        } else {
            (Kind::Open, inner)
        };
        let name = body.split(|c: char| c.is_whitespace()).next().unwrap_or("").to_string();
        if !name.is_empty() {
            out.push(Tag { start: lt, end: gt + 1, kind, name });
        }
    }
    out
}

/// The value of an attribute in a start tag, entities decoded.
fn attr(raw: &str, name: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut from = 0;
    while let Some(found) = raw[from..].find(name).map(|p| p + from) {
        from = found + name.len();
        let before_ok = found > 0 && bytes[found - 1].is_ascii_whitespace();
        let after = raw[from..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let end = value[1..].find(quote)?;
        return Some(unescape(&value[1..1 + end]));
    }
    None
}

/// The start tag with one attribute set, added when it is not there.
fn with_attr(raw: &str, name: &str, value: &str) -> String {
    let bytes = raw.as_bytes();
    let mut from = 0;
    while let Some(found) = raw[from..].find(name).map(|p| p + from) {
        from = found + name.len();
        let before_ok = found > 0 && bytes[found - 1].is_ascii_whitespace();
        let after_ws = raw[from..].len() - raw[from..].trim_start().len();
        let eq = from + after_ws;
        if !before_ok || !raw[eq..].starts_with('=') {
            continue;
        }
        let value_ws = raw[eq + 1..].len() - raw[eq + 1..].trim_start().len();
        let open = eq + 1 + value_ws;
        let Some(quote) = raw[open..].chars().next().filter(|q| *q == '"' || *q == '\'') else { continue };
        let Some(len) = raw[open + 1..].find(quote) else { continue };
        return format!("{}\"{}\"{}", &raw[..open], escape(value), &raw[open + 1 + len + 1..]);
    }
    let cut = if raw.ends_with("/>") { raw.len() - 2 } else { raw.len() - 1 };
    format!("{} {name}=\"{}\"{}", raw[..cut].trim_end(), escape(value), &raw[cut..])
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';').filter(|p| *p <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..semi];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .map(|h| u32::from_str_radix(h, 16).ok())
                .unwrap_or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A simple element (text content only), as the metadata block has them.
#[derive(Clone, Debug)]
struct Element {
    tag: Tag,
    /// The whole element, start tag to end tag.
    start: usize,
    end: usize,
    /// Its text, entities decoded and trimmed.
    text: String,
    /// Where the text sits, when the element is not empty.
    content: Option<(usize, usize)>,
}

/// The child elements of the block between two tag positions.
fn children(text: &str, all: &[Tag], open: usize, close: usize) -> Vec<Element> {
    let mut out = Vec::new();
    let mut i = open + 1;
    while i < close {
        let tag = &all[i];
        match tag.kind {
            Kind::Empty => {
                out.push(Element { tag: tag.clone(), start: tag.start, end: tag.end, text: String::new(), content: None });
                i += 1;
            }
            Kind::Open => {
                // The matching end tag, allowing for nested elements of any kind.
                let mut depth = 1;
                let mut j = i + 1;
                while j < close && depth > 0 {
                    match all[j].kind {
                        Kind::Open => depth += 1,
                        Kind::Close => depth -= 1,
                        Kind::Empty => {}
                    }
                    j += 1;
                }
                let end_tag = &all[j - 1];
                let inner = &text[tag.end..end_tag.start];
                let plain = inner.trim().strip_prefix("<![CDATA[").and_then(|s| s.strip_suffix("]]>")).map(str::to_string);
                out.push(Element {
                    tag: tag.clone(),
                    start: tag.start,
                    end: end_tag.end,
                    text: plain.unwrap_or_else(|| unescape(inner.trim())),
                    content: Some((tag.end, end_tag.start)),
                });
                i = j;
            }
            Kind::Close => i += 1,
        }
    }
    out
}

// ---- The archive and its package document ---------------------------------

struct Item {
    id: String,
    /// As written in the manifest.
    href: String,
    /// The entry it points at.
    path: String,
    media_type: String,
    properties: String,
    tag: Tag,
}

struct Package {
    /// Entry names in archive order.
    names: Vec<String>,
    opf_name: String,
    opf: String,
    all: Vec<Tag>,
    version3: bool,
    package_tag: Tag,
    /// Indexes into `all` of the metadata start and end tags.
    metadata: (usize, usize),
    manifest_close: usize,
    spine_tag: Tag,
    items: Vec<Item>,
    /// Manifest ids in reading order.
    spine: Vec<String>,
}

fn open_archive(bytes: &[u8]) -> anyhow::Result<zip::ZipArchive<Cursor<&[u8]>>> {
    Ok(zip::ZipArchive::new(Cursor::new(bytes))?)
}

fn read_entry(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(name).ok()?;
    let mut out = Vec::new();
    file.read_to_end(&mut out).ok()?;
    Some(out)
}

fn read_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let bytes = read_entry(archive, name)?;
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn dir_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "",
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The entry a reference points at: relative to `base`, without fragment or
/// query. None for references that leave the archive.
fn resolve(base: &str, href: &str) -> Option<String> {
    let href = href.split('#').next().unwrap_or("").split('?').next().unwrap_or("");
    if href.is_empty() {
        return None;
    }
    let scheme_end = href.find(':');
    if scheme_end.is_some_and(|c| href.find('/').is_none_or(|s| c < s)) || href.starts_with("//") {
        return None;
    }
    let decoded = percent_decode(href);
    let joined = if let Some(abs) = decoded.strip_prefix('/') { abs.to_string() } else { format!("{base}{decoded}") };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

fn load(bytes: &[u8]) -> anyhow::Result<Package> {
    let mut archive = open_archive(bytes)?;
    let names: Vec<String> = (0..archive.len()).filter_map(|i| archive.by_index_raw(i).ok().map(|f| f.name().to_string())).collect();
    let from_container = read_text(&mut archive, "META-INF/container.xml").and_then(|c| {
        tags(&c).iter().find(|t| t.local() == "rootfile").and_then(|t| attr(t.raw(&c), "full-path"))
    });
    let opf_name = from_container
        .filter(|n| names.contains(n))
        .or_else(|| names.iter().find(|n| n.to_lowercase().ends_with(".opf")).cloned())
        .ok_or_else(|| anyhow::anyhow!("no package document"))?;
    let opf = read_text(&mut archive, &opf_name).ok_or_else(|| anyhow::anyhow!("unreadable package document"))?;
    let all = tags(&opf);
    let find = |local: &str, kind: Kind| all.iter().position(|t| t.local() == local && t.kind == kind);
    let package_at = find("package", Kind::Open).ok_or_else(|| anyhow::anyhow!("no package element"))?;
    let package_tag = all[package_at].clone();
    let metadata = (
        find("metadata", Kind::Open).ok_or_else(|| anyhow::anyhow!("no metadata element"))?,
        find("metadata", Kind::Close).ok_or_else(|| anyhow::anyhow!("no metadata element"))?,
    );
    let manifest_open = find("manifest", Kind::Open).ok_or_else(|| anyhow::anyhow!("no manifest element"))?;
    let manifest_close = find("manifest", Kind::Close).ok_or_else(|| anyhow::anyhow!("no manifest element"))?;
    let spine_at = find("spine", Kind::Open).or_else(|| find("spine", Kind::Empty)).ok_or_else(|| anyhow::anyhow!("no spine element"))?;
    let base = dir_of(&opf_name).to_string();
    let mut items = Vec::new();
    for tag in &all[manifest_open + 1..manifest_close] {
        if tag.local() != "item" || tag.kind == Kind::Close {
            continue;
        }
        let raw = tag.raw(&opf);
        let (Some(id), Some(href)) = (attr(raw, "id"), attr(raw, "href")) else { continue };
        items.push(Item {
            path: resolve(&base, &href).unwrap_or_default(),
            id,
            href,
            media_type: attr(raw, "media-type").unwrap_or_default(),
            properties: attr(raw, "properties").unwrap_or_default(),
            tag: tag.clone(),
        });
    }
    let spine = all[spine_at..]
        .iter()
        .filter(|t| t.local() == "itemref" && t.kind != Kind::Close)
        .filter_map(|t| attr(t.raw(&opf), "idref"))
        .collect();
    let version3 = attr(package_tag.raw(&opf), "version").is_some_and(|v| v.trim().starts_with('3'));
    Ok(Package {
        names,
        opf_name,
        version3,
        package_tag,
        metadata,
        manifest_close,
        spine_tag: all[spine_at].clone(),
        items,
        spine,
        all,
        opf,
    })
}

impl Package {
    fn base(&self) -> &str {
        dir_of(&self.opf_name)
    }
    fn meta(&self) -> Vec<Element> {
        children(&self.opf, &self.all, self.metadata.0, self.metadata.1)
    }
    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
    /// The prefix that Dublin Core elements carry in this document, and the
    /// namespace declaration a new one needs when nothing declares it.
    fn dc(&self) -> (String, &'static str) {
        let used = self.meta().into_iter().find(|e| matches!(e.tag.local(), "title" | "identifier" | "language" | "creator"));
        let prefix = used.map(|e| e.tag.prefix().to_string()).unwrap_or_else(|| "dc:".to_string());
        let declared = prefix.is_empty() || self.opf.contains(&format!("xmlns:{}=", prefix.trim_end_matches(':')));
        (prefix, if declared { "" } else { " xmlns:dc=\"http://purl.org/dc/elements/1.1/\"" })
    }
    /// The prefix of the package's own elements (`opf:` in some files).
    fn opf_prefix(&self) -> &str {
        self.all[self.metadata.0].prefix()
    }
    /// The declared cover image, by either convention.
    fn cover_item(&self) -> Option<&Item> {
        let by_property = self.items.iter().find(|i| i.properties.split_whitespace().any(|p| p == "cover-image"));
        let by_meta = self
            .meta()
            .iter()
            .filter(|e| e.tag.local() == "meta" && attr(e.tag.raw(&self.opf), "name").as_deref() == Some("cover"))
            .filter_map(|e| attr(e.tag.raw(&self.opf), "content"))
            .find_map(|id| self.items.iter().find(|i| i.id == id));
        by_property.or(by_meta).filter(|i| i.media_type.starts_with("image/") && self.has(&i.path))
    }
    /// An image that is a cover by its name, though nothing declares it.
    fn cover_candidate(&self) -> Option<&Item> {
        self.items.iter().find(|i| {
            i.media_type.starts_with("image/")
                && self.has(&i.path)
                && (i.id.to_lowercase().contains("cover") || i.href.to_lowercase().contains("cover"))
        })
    }
    fn nav_item(&self) -> Option<&Item> {
        self.items.iter().find(|i| i.properties.split_whitespace().any(|p| p == "nav") && self.has(&i.path))
    }
    fn ncx_item(&self) -> Option<&Item> {
        self.items.iter().find(|i| i.media_type == "application/x-dtbncx+xml" && self.has(&i.path))
    }
    /// The content documents in reading order.
    fn spine_items(&self) -> Vec<&Item> {
        self.spine.iter().filter_map(|id| self.items.iter().find(|i| &i.id == id)).filter(|i| self.has(&i.path)).collect()
    }
    fn text_of(&self, local: &str) -> Option<String> {
        self.meta().into_iter().find(|e| e.tag.local() == local && !e.text.is_empty()).map(|e| e.text)
    }
}

/// Replacements in a text, applied back to front so offsets stay valid.
#[derive(Default)]
struct Edits(Vec<(usize, usize, String)>);

impl Edits {
    fn replace(&mut self, start: usize, end: usize, with: impl Into<String>) {
        self.0.push((start, end, with.into()));
    }
    fn insert(&mut self, at: usize, what: impl Into<String>) {
        self.0.push((at, at, what.into()));
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn apply(mut self, text: &str) -> String {
        // Insertions at the same place keep the order they were made in.
        let mut indexed: Vec<(usize, (usize, usize, String))> = self.0.drain(..).enumerate().collect();
        indexed.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(b.0.cmp(&a.0)));
        let mut out = text.to_string();
        for (_, (start, end, with)) in indexed {
            out.replace_range(start..end, &with);
        }
        out
    }
}

/// Changes to the archive: entries replaced or added.
#[derive(Default)]
struct Changes {
    files: BTreeMap<String, Vec<u8>>,
    /// Write a new `mimetype` entry: the file has none, or a wrong one.
    mimetype: bool,
}

impl Changes {
    fn is_empty(&self) -> bool {
        self.files.is_empty() && !self.mimetype
    }
}

/// The archive with the changes made. Untouched entries are copied without
/// recompression.
fn rewrite(bytes: &[u8], changes: &Changes) -> anyhow::Result<Vec<u8>> {
    let mut archive = open_archive(bytes)?;
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut written: HashSet<String> = HashSet::new();
    // `mimetype` goes first and uncompressed, as the format asks.
    if changes.mimetype || archive.by_name("mimetype").is_ok() {
        writer.start_file("mimetype", stored)?;
        writer.write_all(MIMETYPE.as_bytes())?;
        written.insert("mimetype".to_string());
    }
    for i in 0..archive.len() {
        let name = archive.by_index_raw(i)?.name().to_string();
        if !written.insert(name.clone()) {
            continue;
        }
        match changes.files.get(&name) {
            Some(data) => {
                writer.start_file(&name, deflated)?;
                writer.write_all(data)?;
            }
            None => writer.raw_copy_file(archive.by_index_raw(i)?)?,
        }
    }
    for (name, data) in &changes.files {
        if written.insert(name.clone()) {
            writer.start_file(name, deflated)?;
            writer.write_all(data)?;
        }
    }
    Ok(writer.finish()?.into_inner())
}

// ---- The health check ------------------------------------------------------

/// The `mimetype` entry says EPUB. Its place in the archive is put right
/// whenever a file is rewritten, and is not worth reporting on its own.
fn mimetype_ok(bytes: &[u8]) -> bool {
    let Ok(mut archive) = open_archive(bytes) else { return false };
    read_text(&mut archive, "mimetype").is_some_and(|content| content.trim() == MIMETYPE)
}

/// Whether a text is a rights statement, and which way it points: Some(true)
/// for a claim of copyright, Some(false) for a free licence or the public
/// domain, None when it says neither.
fn rights_verdict(text: &str) -> Option<bool> {
    let text = text.to_lowercase();
    const FREE: [&str; 6] = ["public domain", "creative commons", "creativecommons", "cc0", "cc by", "gutenberg"];
    const CLAIM: [&str; 8] = [
        "©",
        "copyright",
        "all rights reserved",
        "alla rättigheter",
        "alle rechte vorbehalten",
        "tous droits réservés",
        "todos los derechos reservados",
        "kaikki oikeudet pidätetään",
    ];
    if FREE.iter().any(|m| text.contains(m)) {
        return Some(false);
    }
    CLAIM.iter().any(|m| text.contains(m)).then_some(true)
}

/// Whether the file itself says it is protected by copyright: in its rights
/// statement, or else on the pages where a copyright notice usually sits.
/// Only this direction is read out of a file. That a book is free is for the
/// owner to state, with a source.
pub fn claims_copyright(bytes: &[u8]) -> bool {
    let Ok(package) = load(bytes) else { return false };
    let stated: Vec<String> = package.meta().into_iter().filter(|e| e.tag.local() == "rights").map(|e| e.text).collect();
    if let Some(verdict) = rights_verdict(&stated.join(" ")) {
        return verdict;
    }
    let Ok(mut archive) = open_archive(bytes) else { return false };
    let spine = package.spine_items();
    let pages = spine.iter().take(8).chain(spine.iter().skip(8).rev().take(3));
    let mut claimed = false;
    for item in pages {
        let Some(text) = read_text(&mut archive, &item.path) else { continue };
        match rights_verdict(&crate::booktext::html_to_text(&text)) {
            Some(false) => return false,
            Some(true) => claimed = true,
            None => {}
        }
    }
    claimed
}

/// Encryption other than font obfuscation, which reading apps undo themselves.
pub fn encrypted(bytes: &[u8]) -> bool {
    let Ok(mut archive) = open_archive(bytes) else { return false };
    let Some(xml) = read_text(&mut archive, "META-INF/encryption.xml") else { return false };
    tags(&xml).iter().filter(|t| t.local() == "EncryptionMethod").filter_map(|t| attr(t.raw(&xml), "Algorithm")).any(|a| {
        a != "http://www.idpf.org/2008/embedding" && a != "http://ns.adobe.com/pdf/enc#RC"
    })
}

/// The language the content documents declare on their root element.
fn content_language(bytes: &[u8], package: &Package) -> Option<String> {
    let mut archive = open_archive(bytes).ok()?;
    for item in package.spine_items().into_iter().take(5) {
        let Some(text) = read_text(&mut archive, &item.path) else { continue };
        let Some(html) = tags(&text).into_iter().find(|t| t.local() == "html") else { continue };
        let raw = html.raw(&text);
        if let Some(lang) = attr(raw, "xml:lang").or_else(|| attr(raw, "lang")).map(|l| l.trim().to_string()) {
            if (2..=35).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Some(lang);
            }
        }
    }
    None
}

/// References from content documents to entries that are not in the archive.
fn broken_links(bytes: &[u8], package: &Package) -> (usize, Vec<String>) {
    let Ok(mut archive) = open_archive(bytes) else { return (0, Vec::new()) };
    let present: HashSet<&str> = package.names.iter().map(String::as_str).collect();
    let mut count = 0;
    let mut examples = Vec::new();
    for item in package.items.iter().filter(|i| i.media_type == "application/xhtml+xml" && present.contains(i.path.as_str())) {
        let Some(text) = read_text(&mut archive, &item.path) else { continue };
        let base = dir_of(&item.path);
        let mut seen: HashSet<String> = HashSet::new();
        for tag in tags(&text) {
            if tag.kind == Kind::Close {
                continue;
            }
            let name = match tag.local() {
                "a" => "href",
                "img" => "src",
                "image" => "xlink:href",
                _ => continue,
            };
            let Some(target) = attr(tag.raw(&text), name).and_then(|h| resolve(base, h.trim())) else { continue };
            if target.is_empty() || present.contains(target.as_str()) || !seen.insert(target.clone()) {
                continue;
            }
            count += 1;
            if examples.len() < EXAMPLES {
                examples.push(format!("{} → {}", item.href, target));
            }
        }
    }
    (count, examples)
}

/// What is wrong with the file. An empty list is a clean bill of health.
pub fn inspect(bytes: &[u8]) -> anyhow::Result<Vec<Issue>> {
    let package = load(bytes)?;
    let mut issues = Vec::new();
    if encrypted(bytes) {
        issues.push(issue("encrypted", false));
    }
    if !mimetype_ok(bytes) {
        issues.push(issue("mimetype", true));
    }
    if package.text_of("title").is_none() {
        issues.push(issue("no_title", false));
    }
    if package.text_of("language").is_none() {
        issues.push(issue("no_language", content_language(bytes, &package).is_some()));
    }
    if package.text_of("identifier").is_none() {
        issues.push(issue("no_identifier", true));
    }
    if package.cover_item().is_none() {
        issues.push(issue("no_cover", package.cover_candidate().is_some()));
    }
    if package.nav_item().is_none() && package.ncx_item().is_none() {
        issues.push(issue("no_toc", !package.spine_items().is_empty()));
    }
    let missing: Vec<&Item> = package.items.iter().filter(|i| !i.path.is_empty() && !package.has(&i.path)).collect();
    if !missing.is_empty() {
        let mut found = issue("missing_files", true);
        found.count = missing.len();
        found.examples = missing.iter().take(EXAMPLES).map(|i| i.href.clone()).collect();
        issues.push(found);
    }
    let (count, examples) = broken_links(bytes, &package);
    if count > 0 {
        let mut found = issue("broken_links", false);
        found.count = count;
        found.examples = examples;
        issues.push(found);
    }
    Ok(issues)
}

// ---- Repairs ---------------------------------------------------------------

/// The heading of a content document: its first h1–h3, else its title.
fn heading(text: &str) -> Option<String> {
    let all = tags(text);
    for wanted in [["h1", "h2", "h3"].as_slice(), ["title"].as_slice()] {
        let Some(open) = all.iter().position(|t| t.kind == Kind::Open && wanted.contains(&t.local().to_lowercase().as_str())) else {
            continue;
        };
        let name = all[open].local().to_lowercase();
        let Some(close) = all[open..].iter().find(|t| t.kind == Kind::Close && t.local().to_lowercase() == name) else { continue };
        let label = crate::booktext::html_to_text(&text[all[open].end..close.start]);
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        if !label.is_empty() {
            return Some(label.chars().take(120).collect());
        }
    }
    None
}

/// (href as in the manifest, label) for a generated table of contents.
fn toc_entries(bytes: &[u8], package: &Package) -> Vec<(String, String)> {
    let Ok(mut archive) = open_archive(bytes) else { return Vec::new() };
    let mut entries: Vec<(String, String)> = Vec::new();
    for item in package.spine_items() {
        let Some(label) = read_text(&mut archive, &item.path).and_then(|t| heading(&t)) else { continue };
        // A book that repeats its title on every page gets one entry for it.
        if entries.last().is_none_or(|(_, last)| *last != label) {
            entries.push((item.href.clone(), label));
        }
    }
    if entries.is_empty() {
        if let Some(first) = package.spine_items().first() {
            entries.push((first.href.clone(), package.text_of("title").unwrap_or_else(|| "Start".to_string())));
        }
    }
    entries
}

fn free_name(package: &Package, stem: &str, ext: &str) -> (String, String) {
    let mut n = 0;
    loop {
        let name = if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") };
        let path = format!("{}{name}", package.base());
        if !package.has(&path) && !package.items.iter().any(|i| i.href == name) {
            return (name, path);
        }
        n += 1;
    }
}

fn free_id(package: &Package, stem: &str) -> String {
    let mut n = 0;
    loop {
        let id = if n == 0 { stem.to_string() } else { format!("{stem}-{n}") };
        if !package.opf.contains(&format!("id=\"{id}\"")) {
            return id;
        }
        n += 1;
    }
}

/// A stable urn:uuid for a file that has no identifier of its own.
fn uuid_for(bytes: &[u8]) -> String {
    let h = crate::books::sha256_hex(bytes);
    format!("urn:uuid:{}-{}-4{}-a{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}

/// Point the cover declaration at a manifest item. A declaration that is
/// already there, pointing at something that is no cover, is redirected.
fn declare_cover(package: &Package, edits: &mut Edits, id: &str) {
    let opf = &package.opf;
    let stale: Vec<Element> = package
        .meta()
        .into_iter()
        .filter(|e| e.tag.local() == "meta" && attr(e.tag.raw(opf), "name").as_deref() == Some("cover"))
        .collect();
    match stale.split_first() {
        Some((first, rest)) => {
            edits.replace(first.tag.start, first.tag.end, with_attr(first.tag.raw(opf), "content", id));
            for e in rest {
                edits.replace(e.start, e.end, "");
            }
        }
        None => edits.insert(
            package.all[package.metadata.1].start,
            format!("<{}meta name=\"cover\" content=\"{}\"/>\n", package.opf_prefix(), escape(id)),
        ),
    }
    for item in package.items.iter().filter(|i| i.id != id && i.properties.split_whitespace().any(|p| p == "cover-image")) {
        let properties = item.properties.split_whitespace().filter(|p| *p != "cover-image").collect::<Vec<_>>().join(" ");
        edits.replace(item.tag.start, item.tag.end, with_attr(item.tag.raw(opf), "properties", &properties));
    }
}

/// Make the repairs that need no decision. Returns the new file, when there
/// was something to repair, and the codes of what was done.
pub fn repair(bytes: &[u8]) -> anyhow::Result<(Option<Vec<u8>>, Vec<String>)> {
    let package = load(bytes)?;
    let mut changes = Changes::default();
    let mut edits = Edits::default();
    let mut fixed: Vec<String> = Vec::new();
    let (dc, dc_ns) = package.dc();
    let opf_prefix = package.opf_prefix().to_string();
    let meta_end = package.all[package.metadata.1].start;
    let manifest_end = package.all[package.manifest_close].start;

    if !mimetype_ok(bytes) {
        changes.mimetype = true;
        fixed.push("mimetype".into());
    }
    if package.text_of("language").is_none() {
        if let Some(lang) = content_language(bytes, &package) {
            edits.insert(meta_end, format!("<{dc}language{dc_ns}>{}</{dc}language>\n", escape(&lang)));
            fixed.push("no_language".into());
        }
    }
    if package.text_of("identifier").is_none() {
        let id = free_id(&package, "legejo-id");
        edits.insert(meta_end, format!("<{dc}identifier{dc_ns} id=\"{id}\">{}</{dc}identifier>\n", uuid_for(bytes)));
        let unique = attr(package.package_tag.raw(&package.opf), "unique-identifier");
        if unique.is_none_or(|u| !package.opf.contains(&format!("id=\"{u}\""))) {
            edits.replace(
                package.package_tag.start,
                package.package_tag.end,
                with_attr(package.package_tag.raw(&package.opf), "unique-identifier", &id),
            );
        }
        fixed.push("no_identifier".into());
    }
    if package.cover_item().is_none() {
        if let Some(item) = package.cover_candidate() {
            declare_cover(&package, &mut edits, &item.id);
            if package.version3 {
                let properties = format!("{} cover-image", item.properties).trim().to_string();
                edits.replace(item.tag.start, item.tag.end, with_attr(item.tag.raw(&package.opf), "properties", &properties));
            }
            fixed.push("no_cover".into());
        }
    }
    if package.nav_item().is_none() && package.ncx_item().is_none() {
        let entries = toc_entries(bytes, &package);
        if !entries.is_empty() {
            let title = package.text_of("title").unwrap_or_default();
            if package.version3 {
                let (name, path) = free_name(&package, "legejo-nav", "xhtml");
                let id = free_id(&package, "legejo-nav");
                let list: String =
                    entries.iter().map(|(href, label)| format!("<li><a href=\"{}\">{}</a></li>\n", escape(href), escape(label))).collect();
                let lang = package.text_of("language").or_else(|| content_language(bytes, &package)).unwrap_or_else(|| "en".into());
                changes.files.insert(
                    path,
                    format!(
                        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n\
                         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"{l}\" xml:lang=\"{l}\">\n\
                         <head><title>{t}</title></head>\n<body>\n<nav epub:type=\"toc\">\n<ol>\n{list}</ol>\n</nav>\n</body>\n</html>\n",
                        l = escape(&lang),
                        t = escape(&title),
                    )
                    .into_bytes(),
                );
                edits.insert(
                    manifest_end,
                    format!("<{opf_prefix}item id=\"{id}\" href=\"{name}\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n"),
                );
            } else {
                let (name, path) = free_name(&package, "legejo-toc", "ncx");
                let id = free_id(&package, "legejo-ncx");
                let points: String = entries
                    .iter()
                    .enumerate()
                    .map(|(i, (href, label))| {
                        format!(
                            "<navPoint id=\"p{n}\" playOrder=\"{n}\"><navLabel><text>{}</text></navLabel><content src=\"{}\"/></navPoint>\n",
                            escape(label),
                            escape(href),
                            n = i + 1
                        )
                    })
                    .collect();
                let uid = package.text_of("identifier").unwrap_or_else(|| uuid_for(bytes));
                changes.files.insert(
                    path,
                    format!(
                        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
                         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
                         <head><meta name=\"dtb:uid\" content=\"{}\"/></head>\n<docTitle><text>{}</text></docTitle>\n<navMap>\n{points}</navMap>\n</ncx>\n",
                        escape(&uid),
                        escape(&title),
                    )
                    .into_bytes(),
                );
                edits.insert(
                    manifest_end,
                    format!("<{opf_prefix}item id=\"{id}\" href=\"{name}\" media-type=\"application/x-dtbncx+xml\"/>\n"),
                );
                edits.replace(package.spine_tag.start, package.spine_tag.end, with_attr(package.spine_tag.raw(&package.opf), "toc", &id));
            }
            fixed.push("no_toc".into());
        }
    }

    // Entries the manifest lists but the archive lacks are taken out of the
    // manifest, and out of the reading order: a reading system that goes
    // through the manifest then finds everything it is told about.
    let missing: Vec<&Item> = package.items.iter().filter(|i| !i.path.is_empty() && !package.has(&i.path)).collect();
    if !missing.is_empty() {
        // An element written with a separate end tag goes as a whole.
        let whole = |at: usize| {
            let tag = &package.all[at];
            let end = match tag.kind {
                Kind::Open => package.all[at..].iter().find(|t| t.kind == Kind::Close && t.name == tag.name).map_or(tag.end, |t| t.end),
                _ => tag.end,
            };
            (tag.start, end)
        };
        for (at, tag) in package.all.iter().enumerate() {
            if tag.kind == Kind::Close {
                continue;
            }
            let raw = tag.raw(&package.opf);
            let gone = match tag.local() {
                "item" => missing.iter().any(|i| i.tag.start == tag.start),
                "itemref" => attr(raw, "idref").is_some_and(|id| missing.iter().any(|i| i.id == id)),
                _ => false,
            };
            if gone {
                let (start, end) = whole(at);
                edits.replace(start, end, "");
            }
        }
        fixed.push("missing_files".into());
    }

    if !edits.is_empty() {
        changes.files.insert(package.opf_name.clone(), edits.apply(&package.opf).into_bytes());
    }
    if changes.is_empty() {
        return Ok((None, fixed));
    }
    let out = rewrite(bytes, &changes)?;
    // A repair must leave a file that still reads.
    load(&out)?;
    Ok((Some(out), fixed))
}

// ---- Metadata ----------------------------------------------------------------

fn number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Several authors in one field are written as one creator each.
fn split_authors(author: &str) -> Vec<String> {
    let parts: Vec<String> = if author.contains(';') {
        author.split(';').map(str::to_string).collect()
    } else if author.contains(" & ") {
        author.split(" & ").map(str::to_string).collect()
    } else {
        vec![author.to_string()]
    };
    parts.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// Write title, authors, language and series into the package document.
/// None when the file already says the same.
pub fn write_metadata(bytes: &[u8], meta: &Meta) -> anyhow::Result<Option<Vec<u8>>> {
    let package = load(bytes)?;
    let opf = &package.opf;
    let elements = package.meta();
    let mut edits = Edits::default();
    let (dc, dc_ns) = package.dc();
    let opf_prefix = package.opf_prefix().to_string();
    let meta_end = package.all[package.metadata.1].start;
    let set_text = |edits: &mut Edits, e: &Element, value: &str| match e.content {
        Some((start, end)) => edits.replace(start, end, escape(value)),
        None => edits.replace(e.start, e.end, format!("{}>{}</{}>", opf[e.start..e.end].trim_end_matches("/>").trim_end(), escape(value), e.tag.name)),
    };
    let of = |local: &str| elements.iter().filter(|e| e.tag.local() == local).collect::<Vec<_>>();

    // Title: the first one is the main title.
    let title = meta.title.trim();
    match of("title").first() {
        Some(e) if e.text == title => {}
        Some(e) => set_text(&mut edits, e, title),
        None => edits.insert(meta_end, format!("<{dc}title{dc_ns}>{}</{dc}title>\n", escape(title))),
    }

    // Authors. The catalog keeps them as one string, joined with ", " when
    // the file has several; a string that still matches means no change.
    let creators = of("creator");
    let current = creators.iter().map(|e| e.text.as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(", ");
    let wanted = meta.author.map(str::trim).unwrap_or("");
    if current != wanted {
        let authors = split_authors(wanted);
        if authors.len() == creators.len() {
            // Same number: keep each element with its role and sort name.
            for (e, a) in creators.iter().zip(&authors) {
                if e.text != *a {
                    set_text(&mut edits, e, a);
                }
            }
        } else {
            for e in &creators {
                edits.replace(e.start, e.end, "");
                // Refinements of a removed creator go with it.
                if let Some(id) = attr(e.tag.raw(opf), "id") {
                    for r in elements.iter().filter(|m| attr(m.tag.raw(opf), "refines").as_deref() == Some(&format!("#{id}"))) {
                        edits.replace(r.start, r.end, "");
                    }
                }
            }
            for a in &authors {
                edits.insert(meta_end, format!("<{dc}creator{dc_ns}>{}</{dc}creator>\n", escape(a)));
            }
        }
    }

    // Language: only ever set, never removed.
    if let Some(language) = meta.language.map(str::trim).filter(|l| !l.is_empty()) {
        match of("language").first() {
            Some(e) if e.text == language => {}
            Some(e) => set_text(&mut edits, e, language),
            None => edits.insert(meta_end, format!("<{dc}language{dc_ns}>{}</{dc}language>\n", escape(language))),
        }
    }

    // Series, in both conventions: calibre's metas, which reading apps read
    // from any EPUB, and belongs-to-collection in EPUB 3.
    let metas = of("meta");
    let named = |name: &str| metas.iter().copied().find(|e| attr(e.tag.raw(opf), "name").as_deref() == Some(name));
    let property = |name: &str| metas.iter().copied().filter(|e| attr(e.tag.raw(opf), "property").as_deref() == Some(name)).collect::<Vec<_>>();
    let collection = property("belongs-to-collection").into_iter().next();
    let collection_id = collection.and_then(|e| attr(e.tag.raw(opf), "id"));
    let refining = |prop: &str| {
        let target = collection_id.as_ref().map(|id| format!("#{id}"));
        property(prop).into_iter().find(|e| target.is_some() && attr(e.tag.raw(opf), "refines") == target)
    };
    let series = meta.series.map(str::trim).filter(|s| !s.is_empty());
    let index = series.and(meta.series_index);
    let current_series = named("calibre:series").and_then(|e| attr(e.tag.raw(opf), "content")).filter(|s| !s.trim().is_empty());
    let current_index = named("calibre:series_index").and_then(|e| attr(e.tag.raw(opf), "content")).and_then(|v| v.parse::<f64>().ok());
    let set_named = |edits: &mut Edits, name: &str, value: Option<String>| match (named(name), value) {
        (Some(e), Some(v)) => {
            if attr(e.tag.raw(opf), "content").as_deref() != Some(v.as_str()) {
                edits.replace(e.tag.start, e.tag.end, with_attr(e.tag.raw(opf), "content", &v));
            }
        }
        (Some(e), None) => edits.replace(e.start, e.end, ""),
        (None, Some(v)) => edits.insert(meta_end, format!("<{opf_prefix}meta name=\"{name}\" content=\"{}\"/>\n", escape(&v))),
        (None, None) => {}
    };
    let calibre_differs = current_series.as_deref().map(str::trim) != series || (series.is_some() && current_index != index);
    if calibre_differs {
        set_named(&mut edits, "calibre:series", series.map(str::to_string));
        set_named(&mut edits, "calibre:series_index", index.map(number));
    }
    match (collection, series) {
        (Some(e), Some(name)) => {
            if e.text != name {
                set_text(&mut edits, e, name);
            }
            match (refining("group-position"), index) {
                (Some(p), Some(i)) if p.text.parse::<f64>().ok() != Some(i) => set_text(&mut edits, p, &number(i)),
                (Some(_), Some(_)) => {}
                (Some(p), None) => edits.replace(p.start, p.end, ""),
                (None, Some(i)) => {
                    if let Some(id) = &collection_id {
                        edits.insert(meta_end, format!("<{opf_prefix}meta refines=\"#{id}\" property=\"group-position\">{}</{opf_prefix}meta>\n", number(i)));
                    }
                }
                (None, None) => {}
            }
        }
        (Some(e), None) => {
            edits.replace(e.start, e.end, "");
            for prop in ["group-position", "collection-type"] {
                if let Some(r) = refining(prop) {
                    edits.replace(r.start, r.end, "");
                }
            }
        }
        (None, Some(name)) if package.version3 => {
            let id = free_id(&package, "legejo-series");
            let mut block = format!(
                "<{opf_prefix}meta property=\"belongs-to-collection\" id=\"{id}\">{}</{opf_prefix}meta>\n\
                 <{opf_prefix}meta refines=\"#{id}\" property=\"collection-type\">series</{opf_prefix}meta>\n",
                escape(name)
            );
            if let Some(i) = index {
                block.push_str(&format!("<{opf_prefix}meta refines=\"#{id}\" property=\"group-position\">{}</{opf_prefix}meta>\n", number(i)));
            }
            edits.insert(meta_end, block);
        }
        _ => {}
    }

    if edits.is_empty() {
        return Ok(None);
    }
    let mut changes = Changes::default();
    changes.files.insert(package.opf_name.clone(), edits.apply(opf).into_bytes());
    let out = rewrite(bytes, &changes)?;
    load(&out)?;
    Ok(Some(out))
}

// ---- The cover ---------------------------------------------------------------

fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

/// Whether the file declares a cover image that is in it.
pub fn has_cover(bytes: &[u8]) -> bool {
    load(bytes).is_ok_and(|package| package.cover_item().is_some())
}

/// Put a cover image into the file: in place of the declared one, or as a
/// new declared cover when the file has none.
pub fn set_cover(bytes: &[u8], image: &[u8], mime: &str) -> anyhow::Result<Vec<u8>> {
    let package = load(bytes)?;
    let mut changes = Changes::default();
    let mut edits = Edits::default();
    match package.cover_item() {
        Some(item) => {
            changes.files.insert(item.path.clone(), image.to_vec());
            if item.media_type != mime {
                edits.replace(item.tag.start, item.tag.end, with_attr(item.tag.raw(&package.opf), "media-type", mime));
            }
        }
        None => {
            let opf_prefix = package.opf_prefix();
            let (name, path) = free_name(&package, "legejo-cover", extension(mime));
            let id = free_id(&package, "legejo-cover");
            let properties = if package.version3 { " properties=\"cover-image\"" } else { "" };
            changes.files.insert(path, image.to_vec());
            edits.insert(
                package.all[package.manifest_close].start,
                format!("<{opf_prefix}item id=\"{id}\" href=\"{name}\" media-type=\"{}\"{properties}/>\n", escape(mime)),
            );
            declare_cover(&package, &mut edits, &id);
        }
    }
    if !edits.is_empty() {
        changes.files.insert(package.opf_name.clone(), edits.apply(&package.opf).into_bytes());
    }
    let out = rewrite(bytes, &changes)?;
    load(&out)?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small EPUB built from parts, for tests here and elsewhere.
    pub(crate) fn build(opf: &str, files: &[(&str, &str)], mimetype_first: bool) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if mimetype_first {
            zip.start_file("mimetype", stored).unwrap();
            zip.write_all(MIMETYPE.as_bytes()).unwrap();
        }
        zip.start_file("META-INF/container.xml", deflated).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        )
        .unwrap();
        zip.start_file("OEBPS/content.opf", deflated).unwrap();
        zip.write_all(opf.as_bytes()).unwrap();
        for (name, content) in files {
            zip.start_file(format!("OEBPS/{name}"), deflated).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        if !mimetype_first {
            zip.start_file("mimetype", deflated).unwrap();
            zip.write_all(MIMETYPE.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const CHAPTER: &str = r##"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml" xml:lang="sv"><head><title>Kapitel</title></head><body><h1>Första <em>kapitlet</em></h1><p>Text. <a href="two.xhtml#x">Vidare</a> <a href="https://example.org/">ut</a> <a href="#here">hit</a></p></body></html>"##;
    const CHAPTER2: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Två</title></head><body><h2>Andra kapitlet</h2><p><img src="img/saknas.png" alt=""/><a href="borta.xhtml">x</a><a href="borta.xhtml">y</a></p></body></html>"#;

    /// An EPUB 3 with everything in order.
    fn healthy() -> Vec<u8> {
        let opf = r##"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">urn:isbn:9789100000000</dc:identifier>
    <dc:title>R&#246;da rummet</dc:title>
    <dc:creator id="c1">August Strindberg</dc:creator>
    <meta refines="#c1" property="file-as">Strindberg, August</meta>
    <dc:language>sv</dc:language>
    <meta name="cover" content="cover"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="one" href="one.xhtml" media-type="application/xhtml+xml"/>
    <item id="two" href="two.xhtml" media-type="application/xhtml+xml"/>
    <item id="cover" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/>
  </manifest>
  <spine><itemref idref="one"/><itemref idref="two"/></spine>
</package>"##;
        let two = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h2>Andra kapitlet</h2></body></html>"#;
        build(opf, &[("nav.xhtml", "<html/>"), ("one.xhtml", CHAPTER), ("two.xhtml", two), ("cover.jpg", "jpeg")], true)
    }

    /// An EPUB 2 with most things missing.
    fn ailing() -> Vec<u8> {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Utan allt</dc:title>
    <!-- <dc:language>en</dc:language> -->
  </metadata>
  <manifest>
    <item id="one" href="one.xhtml" media-type="application/xhtml+xml"/>
    <item id="two" href="two.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="Images/Cover.png" media-type="image/png"/>
    <item id="gone" href="gone.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="one"/>
    <itemref idref="two"/>
  </spine>
</package>"#;
        build(opf, &[("one.xhtml", CHAPTER), ("two.xhtml", CHAPTER2), ("Images/Cover.png", "png")], false)
    }

    fn mimetype_first(bytes: &[u8]) -> bool {
        let mut archive = open_archive(bytes).unwrap();
        let first = archive.by_index(0).unwrap();
        first.name() == "mimetype" && first.compression() == zip::CompressionMethod::Stored
    }

    fn codes(issues: &[Issue]) -> Vec<&str> {
        issues.iter().map(|i| i.code.as_str()).collect()
    }

    #[test]
    fn a_healthy_file_has_no_issues_and_is_left_alone() {
        let bytes = healthy();
        assert_eq!(inspect(&bytes).unwrap(), Vec::new());
        let (out, fixed) = repair(&bytes).unwrap();
        assert!(out.is_none() && fixed.is_empty());
    }

    #[test]
    fn issues_are_found_and_the_fixable_ones_repaired() {
        let bytes = ailing();
        let issues = inspect(&bytes).unwrap();
        assert_eq!(codes(&issues), ["no_language", "no_identifier", "no_cover", "no_toc", "missing_files", "broken_links"]);
        let by = |code: &str| issues.iter().find(|i| i.code == code).unwrap().clone();
        assert!(by("no_language").fixable && by("no_cover").fixable && by("no_toc").fixable && by("missing_files").fixable);
        assert_eq!((by("missing_files").count, by("missing_files").examples), (1, vec!["gone.css".to_string()]));
        // The same missing file linked twice counts once per document; links
        // out of the book and within the page are not checked.
        assert_eq!(by("broken_links").count, 2);
        assert_eq!(by("broken_links").examples, ["two.xhtml → OEBPS/img/saknas.png", "two.xhtml → OEBPS/borta.xhtml"]);

        let (out, fixed) = repair(&bytes).unwrap();
        let out = out.unwrap();
        assert_eq!(fixed, ["no_language", "no_identifier", "no_cover", "no_toc", "missing_files"]);
        // A rewritten file has `mimetype` first and uncompressed, as the format asks.
        assert!(!mimetype_first(&bytes) && mimetype_first(&out));
        assert_eq!(codes(&inspect(&out).unwrap()), ["broken_links"]);
        assert!(!load(&out).unwrap().opf.contains("gone.css"));
        // Repairing again changes nothing.
        assert!(repair(&out).unwrap().0.is_none());

        let package = load(&out).unwrap();
        assert_eq!(package.text_of("language").as_deref(), Some("sv"));
        assert!(package.text_of("identifier").unwrap().starts_with("urn:uuid:"));
        assert_eq!(package.cover_item().unwrap().href, "Images/Cover.png");
        let ncx = package.ncx_item().unwrap();
        assert_eq!(attr(package.spine_tag.raw(&package.opf), "toc").as_deref(), Some(ncx.id.as_str()));
        let mut archive = open_archive(&out).unwrap();
        let toc = read_text(&mut archive, &ncx.path).unwrap();
        assert!(toc.contains("<text>Första kapitlet</text>") && toc.contains("src=\"two.xhtml\"") && toc.contains("Andra kapitlet"));
        // The commented-out language stayed a comment.
        assert!(package.opf.contains("<!-- <dc:language>en</dc:language> -->"));
        // The epub crate reads the repaired file, with its contents.
        let doc = epub::doc::EpubDoc::from_reader(Cursor::new(out.clone())).unwrap();
        assert_eq!(doc.toc.len(), 2);
    }

    #[test]
    fn an_epub3_without_navigation_gets_a_nav_document() {
        let bytes = healthy();
        let package = load(&bytes).unwrap();
        let nav = package.nav_item().unwrap();
        let opf = package.opf.replace(&package.opf[nav.tag.start..nav.tag.end], "");
        let two = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h2>Andra &amp; sista</h2></body></html>"#;
        let bytes = build(&opf, &[("one.xhtml", CHAPTER), ("two.xhtml", two), ("cover.jpg", "jpeg")], true);
        assert_eq!(codes(&inspect(&bytes).unwrap()), ["no_toc"]);
        let (out, fixed) = repair(&bytes).unwrap();
        let out = out.unwrap();
        assert_eq!(fixed, ["no_toc"]);
        assert_eq!(inspect(&out).unwrap(), Vec::new());
        let package = load(&out).unwrap();
        let mut archive = open_archive(&out).unwrap();
        let nav = read_text(&mut archive, &package.nav_item().unwrap().path).unwrap();
        assert!(nav.contains("<a href=\"one.xhtml\">Första kapitlet</a>") && nav.contains("<a href=\"two.xhtml\">Andra &amp; sista</a>"));
    }

    #[test]
    fn metadata_is_written_and_reads_back() {
        let bytes = healthy();
        let same = Meta { title: "Röda rummet", author: Some("August Strindberg"), language: Some("sv"), series: None, series_index: None };
        assert!(write_metadata(&bytes, &same).unwrap().is_none(), "nothing to change");

        let meta = Meta { title: "Röda rummet & annat", author: Some("A. Strindberg"), language: Some("sv-SE"), series: Some("Samlade <verk>"), series_index: Some(2.5) };
        let out = write_metadata(&bytes, &meta).unwrap().unwrap();
        assert!(write_metadata(&out, &meta).unwrap().is_none(), "writing twice changes nothing");
        let package = load(&out).unwrap();
        assert_eq!(package.text_of("title").as_deref(), Some("Röda rummet & annat"));
        assert_eq!(package.text_of("language").as_deref(), Some("sv-SE"));
        // One author for one creator: the element and its sort name are kept.
        assert!(package.opf.contains("<dc:creator id=\"c1\">A. Strindberg</dc:creator>") && package.opf.contains("Strindberg, August"));
        assert!(package.opf.contains("name=\"calibre:series\" content=\"Samlade &lt;verk&gt;\"") && package.opf.contains("content=\"2.5\""));
        assert!(package.opf.contains("property=\"belongs-to-collection\"") && package.opf.contains("property=\"group-position\">2.5<"));
        // The other entries are untouched, and mimetype is still first.
        assert!(mimetype_first(&out));
        let mut archive = open_archive(&out).unwrap();
        assert_eq!(read_text(&mut archive, "OEBPS/one.xhtml").as_deref(), Some(CHAPTER));

        // Two authors replace one, and the refinement of the old one goes.
        let two = Meta { author: Some("August Strindberg & Siri von Essen"), ..meta };
        let out2 = write_metadata(&out, &two).unwrap().unwrap();
        let package = load(&out2).unwrap();
        let creators: Vec<String> = package.meta().into_iter().filter(|e| e.tag.local() == "creator").map(|e| e.text).collect();
        assert_eq!(creators, ["August Strindberg", "Siri von Essen"]);
        assert!(!package.opf.contains("file-as"));
        // The catalog reads several creators back joined: that is no change.
        let joined = Meta { author: Some("August Strindberg, Siri von Essen"), ..two };
        assert!(write_metadata(&out2, &joined).unwrap().is_none());

        // Leaving the series removes it in both conventions.
        let none = Meta { series: None, series_index: None, ..joined };
        let out3 = write_metadata(&out2, &none).unwrap().unwrap();
        let opf = load(&out3).unwrap().opf;
        assert!(!opf.contains("calibre:series") && !opf.contains("belongs-to-collection") && !opf.contains("group-position") && !opf.contains("collection-type"));
        assert!(write_metadata(&out3, &none).unwrap().is_none());
    }

    #[test]
    fn metadata_goes_into_a_file_that_lacks_the_elements() {
        let opf = r#"<opf:package xmlns:opf="http://www.idpf.org/2007/opf" version="2.0"><opf:metadata></opf:metadata><opf:manifest><opf:item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></opf:manifest><opf:spine><opf:itemref idref="one"/></opf:spine></opf:package>"#;
        let bytes = build(opf, &[("one.xhtml", CHAPTER)], true);
        let meta = Meta { title: "Ny titel", author: Some("Någon"), language: Some("sv"), series: Some("Serien"), series_index: Some(3.0) };
        let out = write_metadata(&bytes, &meta).unwrap().unwrap();
        let package = load(&out).unwrap();
        assert!(package.opf.contains("<dc:title xmlns:dc=\"http://purl.org/dc/elements/1.1/\">Ny titel</dc:title>"));
        assert!(package.opf.contains("<opf:meta name=\"calibre:series_index\" content=\"3\"/>"));
        assert!(!package.opf.contains("belongs-to-collection"), "EPUB 2 gets the calibre metas only");
        assert!(write_metadata(&out, &meta).unwrap().is_none());
    }

    #[test]
    fn a_cover_is_replaced_or_added() {
        let out = set_cover(&healthy(), b"new png", "image/png").unwrap();
        let package = load(&out).unwrap();
        let cover = package.cover_item().unwrap();
        assert_eq!((cover.href.as_str(), cover.media_type.as_str()), ("cover.jpg", "image/png"));
        assert_eq!(read_entry(&mut open_archive(&out).unwrap(), &cover.path).unwrap(), b"new png");

        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/></spine></package>"#;
        let bare = build(opf, &[("one.xhtml", CHAPTER)], true);
        assert!(codes(&inspect(&bare).unwrap()).contains(&"no_cover"));
        let out = set_cover(&bare, b"jpeg bytes", "image/jpeg").unwrap();
        assert!(!codes(&inspect(&out).unwrap()).contains(&"no_cover"));
        let mut doc = epub::doc::EpubDoc::from_reader(Cursor::new(out)).unwrap();
        assert_eq!(doc.get_cover().unwrap().0, b"jpeg bytes");
    }

    #[test]
    fn a_stale_cover_declaration_is_redirected() {
        // The declared cover is not in the archive; an uploaded one takes over.
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title><meta name="cover" content="old"/></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="old" href="gone.png" media-type="image/png" properties="cover-image"/></manifest><spine><itemref idref="one"/></spine></package>"#;
        let bytes = build(opf, &[("one.xhtml", CHAPTER)], true);
        assert!(codes(&inspect(&bytes).unwrap()).contains(&"no_cover"));
        let out = set_cover(&bytes, b"jpeg bytes", "image/jpeg").unwrap();
        let package = load(&out).unwrap();
        assert_eq!(package.cover_item().unwrap().href, "legejo-cover.jpg");
        assert_eq!(package.opf.matches("name=\"cover\"").count(), 1);
        let mut doc = epub::doc::EpubDoc::from_reader(Cursor::new(out)).unwrap();
        assert_eq!(doc.get_cover().unwrap().0, b"jpeg bytes");
    }

    #[test]
    fn a_missing_mimetype_entry_is_added() {
        let whole = healthy();
        let mut archive = open_archive(&whole).unwrap();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..archive.len() {
            if archive.by_index_raw(i).unwrap().name() != "mimetype" {
                zip.raw_copy_file(archive.by_index_raw(i).unwrap()).unwrap();
            }
        }
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(codes(&inspect(&bytes).unwrap()), ["mimetype"]);
        let (out, fixed) = repair(&bytes).unwrap();
        assert_eq!(fixed, ["mimetype"]);
        assert!(mimetype_first(&out.unwrap()));
    }

    #[test]
    fn a_claim_of_copyright_is_read_from_the_file() {
        let with = |rights: &str, page: &str| {
            let opf = format!(
                r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title>{rights}</metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/></spine></package>"#
            );
            let page = format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{page}</p></body></html>"#);
            build(&opf, &[("one.xhtml", &page)], true)
        };
        assert!(claims_copyright(&with("<dc:rights>Copyright © 2006 by Hampton Sides</dc:rights>", "Text")));
        assert!(claims_copyright(&with("<dc:rights>All rights reserved.</dc:rights>", "Text")));
        assert!(!claims_copyright(&with("<dc:rights>Public domain in the USA.</dc:rights>", "Copyright laws are changing.")));
        assert!(!claims_copyright(&with("<dc:rights>© 2020 A. Author, CC BY 4.0</dc:rights>", "Text")));
        // Without a statement that says either, the pages decide.
        assert!(claims_copyright(&with("<dc:rights>Sven Delblancs efterlevande 1981</dc:rights>", "&#169; N&#229;gon 1981. Alla r&#228;ttigheter f&#246;rbeh&#229;llna.")));
        assert!(claims_copyright(&with("", "First published 2006. All rights reserved.")));
        assert!(!claims_copyright(&with("", "This eBook is for the use of anyone anywhere. Project Gutenberg. Copyright status: free.")));
        assert!(!claims_copyright(&with("", "Det var en gång.")));
    }

    #[test]
    fn tags_attributes_and_references() {
        let raw = r#"<item id="a" href='x&amp;y.xhtml' media-type="t"/>"#;
        assert_eq!(attr(raw, "href").as_deref(), Some("x&y.xhtml"));
        assert_eq!(attr(raw, "ref"), None);
        assert_eq!(with_attr(raw, "href", "z"), r#"<item id="a" href="z" media-type="t"/>"#);
        assert_eq!(with_attr("<spine>", "toc", "ncx"), r#"<spine toc="ncx">"#);
        assert_eq!(with_attr("<item id=\"a\" />", "properties", "nav"), r#"<item id="a" properties="nav"/>"#);
        let found = tags("<a x='>'><!-- <b> --><![CDATA[<c>]]><d/></a>");
        assert_eq!(found.iter().map(|t| (t.name.as_str(), t.kind)).collect::<Vec<_>>(), [("a", Kind::Open), ("d", Kind::Empty), ("a", Kind::Close)]);
        assert_eq!(resolve("OEBPS/text/", "../img/a%20b.png#f").as_deref(), Some("OEBPS/img/a b.png"));
        assert_eq!(resolve("OEBPS/", "https://example.org/x"), None);
        assert_eq!(resolve("OEBPS/", "mailto:a@b.c"), None);
        assert_eq!(resolve("OEBPS/", "#only"), None);
        assert_eq!(unescape("a &amp; b &#229; &#xE4; &nope; &"), "a & b å ä &nope; &");
        assert_eq!(DC_NS, "http://purl.org/dc/elements/1.1/");
    }

    #[test]
    fn encryption_other_than_font_obfuscation_is_reported() {
        let opf = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/></spine></package>"#;
        let with = |algorithm: &str| {
            let base = build(opf, &[("one.xhtml", CHAPTER)], true);
            let mut changes = Changes::default();
            changes.files.insert(
                "META-INF/encryption.xml".into(),
                format!("<encryption><EncryptedData><EncryptionMethod Algorithm=\"{algorithm}\"/></EncryptedData></encryption>").into_bytes(),
            );
            rewrite(&base, &changes).unwrap()
        };
        assert!(!codes(&inspect(&with("http://www.idpf.org/2008/embedding")).unwrap()).contains(&"encrypted"));
        assert!(codes(&inspect(&with("http://www.w3.org/2001/04/xmlenc#aes128-cbc")).unwrap()).contains(&"encrypted"));
    }
}
