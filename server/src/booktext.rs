//! Plain text out of a book: an EPUB's sections in reading order, with titles
//! from the table of contents, or a PDF's pages, for the search inside the
//! books (fulltext.rs) and the tools that let an assistant read and search a
//! book (mcp.rs).

use crate::formats::Format;
use epub::doc::{EpubDoc, NavPoint};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// The largest PDF whose text is read: all of it is held in memory meanwhile.
const PDF_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// One spine item or PDF page with text, in reading order.
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
    place_sections(&mut out);
    Ok(out)
}

/// The text of the file of a book, as far as its format has any.
pub fn read_sections(format: Format, path: &Path) -> anyhow::Result<Vec<Section>> {
    if format == Format::Pdf {
        let size = std::fs::metadata(path)?.len();
        anyhow::ensure!(size <= PDF_MAX_BYTES, "a PDF of {size} bytes is too large to read the text of");
    }
    let bytes = std::fs::read(path)?;
    match format {
        Format::Pdf => pdf_sections(&bytes),
        _ => sections(bytes),
    }
}

/// A PDF's text layer, page by page: what the file was set in, or what OCR
/// laid over a scan. Pages without one are left out, so a scan with no OCR
/// has no sections.
pub fn pdf_sections(bytes: &[u8]) -> anyhow::Result<Vec<Section>> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|e| anyhow::anyhow!("could not read the pdf: {e}"))?;
    let mut out: Vec<Section> = pages
        .iter()
        .map(|page| pdf_page_text(page))
        .filter(|text| text.chars().count() >= 2)
        .map(|text| Section { title: None, text, start: 0.0, end: 0.0 })
        .collect();
    place_sections(&mut out);
    Ok(out)
}

/// A page as it comes out of a PDF to text to search in: no control
/// characters (which Postgres refuses, and scanned books hold), no
/// ligatures, and the words a line end split whole again.
fn pdf_page_text(page: &str) -> String {
    const LIGATURES: [(char, &str); 7] =
        [('\u{FB00}', "ff"), ('\u{FB01}', "fi"), ('\u{FB02}', "fl"), ('\u{FB03}', "ffi"), ('\u{FB04}', "ffl"), ('\u{FB05}', "st"), ('\u{FB06}', "st")];
    let mut text = String::with_capacity(page.len());
    for c in page.chars() {
        match LIGATURES.iter().find(|(ligature, _)| *ligature == c) {
            Some((_, letters)) => text.push_str(letters),
            None if c == '\n' => text.push(c),
            None if c.is_control() => text.push(' '),
            None => text.push(c),
        }
    }
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        let soft = out.ends_with('\u{AD}');
        let hard = out.ends_with('-') && out.chars().rev().nth(1).is_some_and(char::is_alphabetic);
        if (soft || hard) && line.starts_with(char::is_lowercase) {
            out.pop();
            out.push_str(&line);
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&line);
        }
    }
    out.replace('\u{AD}', "")
}

/// Where each section starts and ends in the book, by text length.
fn place_sections(sections: &mut [Section]) {
    let total: usize = sections.iter().map(|s| s.text.len()).sum();
    let mut seen = 0usize;
    for section in sections {
        section.start = if total == 0 { 0.0 } else { seen as f64 / total as f64 };
        seen += section.text.len();
        section.end = if total == 0 { 1.0 } else { seen as f64 / total as f64 };
    }
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

/// Text for HTML or XML: the characters that would otherwise be read as
/// markup, escaped. Used for feeds, pages and mail. The opposite of
/// decode_entities.
pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Text with its character references decoded.
pub(crate) fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    push_text(&mut out, text);
    out
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

/// A PDF as OCR leaves it: on each page, invisible text in a font without
/// glyphs, whose codes only a ToUnicode map explains.
#[cfg(test)]
pub(crate) fn ocr_pdf(pages: &[&str]) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let to_unicode = doc.add_object(Stream::new(
        dictionary! {},
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
          1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
          1 beginbfrange <0000> <FFFF> <0000> endbfrange\n\
          endcmap end end"
            .to_vec(),
    ));
    let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => "GlyphLessFont", "Flags" => 5,
        "FontBBox" => vec![0.into(), 0.into(), 500.into(), 1000.into()],
        "ItalicAngle" => 0, "Ascent" => 1000, "Descent" => 0, "CapHeight" => 1000, "StemV" => 80,
    });
    let cid_font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "GlyphLessFont",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "CIDToGIDMap" => "Identity", "DW" => 500, "FontDescriptor" => descriptor,
    });
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "GlyphLessFont", "Encoding" => "Identity-H",
        "DescendantFonts" => vec![cid_font.into()], "ToUnicode" => to_unicode,
    });
    let mut kids: Vec<Object> = Vec::new();
    for text in pages {
        let hex: String = text.encode_utf16().map(|unit| format!("{unit:04X}")).collect();
        let content = format!("BT 3 Tr /F1 12 Tf 72 720 Td <{hex}> Tj ET");
        let content = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_ocr_laid_over_a_scan_is_found_page_by_page() {
        let found = pdf_sections(&ocr_pdf(&["Första sidans åäö", "", "Tredje sidan"])).unwrap();
        let texts: Vec<&str> = found.iter().map(|s| s.text.trim()).collect();
        assert_eq!(texts, ["Första sidans åäö", "Tredje sidan"], "the empty page is left out");
        assert_eq!(found[0].start, 0.0);
        assert_eq!(found[1].end, 1.0);
    }

    #[test]
    fn a_pdf_page_becomes_text_to_search_in() {
        assert_eq!(pdf_page_text("  en me\u{AD} \ndelålders man \n\n  med ord-\nning\n"), "en medelålders man\nmed ordning");
        assert_eq!(pdf_page_text("Nord-\nSverige\n1914-\n1918"), "Nord-\nSverige\n1914-\n1918");
        assert_eq!(pdf_page_text("en \u{FB01}n \u{0}bok\tsida"), "en fin bok sida");
    }

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
