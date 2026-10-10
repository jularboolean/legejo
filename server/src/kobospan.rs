//! Where a highlight made in the web reader is in the KEPUB the device
//! reads. kepubify wraps every sentence in `<span class="koboSpan"
//! id="kobo.<paragraph>.<sentence>">`, and the device places a highlight by
//! such a span and a character offset into its text. A web highlight has a
//! CFI, which names the spine item, and its text; the text is looked for
//! among the spans of that chapter.

use epub::doc::EpubDoc;
use serde_json::{json, Value};
use std::io::Cursor;

/// A highlight placed by KEPUB spans, the way the device writes a location.
pub struct Placed {
    /// The chapter file, by its path in the archive.
    pub chapter: String,
    pub title: Option<String>,
    pub start_path: String,
    pub start_char: usize,
    pub end_path: String,
    pub end_char: usize,
    /// How far into the chapter it starts, 0–1.
    pub progress: f64,
    /// The text as it stands in the KEPUB, which is how the device quotes it.
    pub text: String,
}

impl Placed {
    pub fn location(&self) -> Value {
        let mut span = json!({
            "chapterFilename": self.chapter,
            "chapterProgress": self.progress,
            "startPath": self.start_path,
            "startChar": self.start_char,
            "endPath": self.end_path,
            "endChar": self.end_char,
        });
        if let Some(title) = &self.title {
            span["chapterTitle"] = json!(title);
        }
        json!({ "span": span })
    }
}

/// The spine item a CFI starts in: its idref when the CFI names it, and its
/// position (the CFI's step is twice the position plus two).
pub fn spine_item_of(cfi: &str) -> Option<(Option<String>, Option<usize>)> {
    let inner = cfi.trim().strip_prefix("epubcfi(")?.trim_end_matches(')');
    let chapter = inner.split('!').next()?;
    let step = chapter.rsplit('/').next()?;
    let (number, id) = match step.split_once('[') {
        Some((number, id)) => (number, Some(id.trim_end_matches(']').trim().to_string()).filter(|id| !id.is_empty())),
        None => (step, None),
    };
    let n: usize = number.trim().parse().ok()?;
    let index = (n >= 2 && n % 2 == 0).then(|| n / 2 - 1);
    Some((id, index))
}

/// Where `text` is in the chapter the CFI names, in this KEPUB. None when
/// the chapter or the text is not there.
pub fn place(kepub: &[u8], cfi: &str, text: &str) -> anyhow::Result<Option<Placed>> {
    let Some((idref, index)) = spine_item_of(cfi) else { return Ok(None) };
    let mut doc = EpubDoc::from_reader(Cursor::new(kepub)).map_err(|e| anyhow::anyhow!("could not read the kepub: {e}"))?;
    let item = idref
        .and_then(|id| doc.spine.iter().find(|s| s.idref == id).cloned())
        .or_else(|| index.and_then(|i| doc.spine.get(i).cloned()));
    let Some(item) = item else { return Ok(None) };
    let Some(path) = doc.resources.get(&item.idref).map(|r| r.path.to_string_lossy().replace('\\', "/")) else { return Ok(None) };
    let Some((html, _)) = doc.get_resource_str(&item.idref) else { return Ok(None) };
    Ok(find_in_chapter(&html, text).map(|found| Placed {
        chapter: path,
        title: title_of(&html),
        start_path: found.start_path,
        start_char: found.start_char,
        end_path: found.end_path,
        end_char: found.end_char,
        progress: found.progress,
        text: found.text,
    }))
}

struct Span {
    id: String,
    text: String,
}

struct Found {
    start_path: String,
    start_char: usize,
    end_path: String,
    end_char: usize,
    progress: f64,
    text: String,
}

/// The value of an attribute in a tag's text.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().last();
        let after = &rest[at + name.len()..];
        if before.is_some_and(char::is_whitespace) && after.trim_start().starts_with('=') {
            let value = after.trim_start()[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value = &value[1..];
                return value.find(quote).map(|end| value[..end].to_string());
            }
            return Some(value.split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("").to_string());
        }
        rest = &rest[at + name.len()..];
    }
    None
}

/// The chapter's kobo spans in order, each with its text as the device
/// counts it (any markup dropped, character references decoded).
fn spans_of(html: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<span") {
        rest = &rest[at..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        let Some(id) = attr(tag, "id").filter(|id| id.starts_with("kobo.")) else { continue };
        if !attr(tag, "class").is_some_and(|c| c.split_whitespace().any(|c| c == "koboSpan")) {
            continue;
        }
        let Some(end) = rest.find("</span>") else { break };
        let inner = &rest[..end];
        rest = &rest[end + "</span>".len()..];
        out.push(Span { id, text: crate::booktext::decode_entities(&strip_tags(inner)) });
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = match rest[lt..].find('>') {
            Some(gt) => &rest[lt + gt + 1..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn title_of(html: &str) -> Option<String> {
    let start = html.find("<title")?;
    let rest = &html[start..];
    let gt = rest.find('>')?;
    let rest = &rest[gt + 1..];
    let end = rest.find("</title")?;
    let title = crate::booktext::decode_entities(&strip_tags(&rest[..end])).split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

/// The selector the device writes for a span: `span#kobo\.4\.1`.
fn selector(id: &str) -> String {
    format!("span#{}", id.replace('.', "\\."))
}

/// The text between two places in the spans, as the device would quote it.
fn text_between(spans: &[Span], from: (usize, usize), to: (usize, usize)) -> String {
    let slice = |text: &str, start: usize, end: usize| -> String {
        let mut out = String::new();
        let mut at = 0;
        for c in text.chars() {
            if at >= start && at < end {
                out.push(c);
            }
            at += c.len_utf16();
        }
        out
    };
    let mut out = String::new();
    for (i, span) in spans.iter().enumerate().take(to.0 + 1).skip(from.0) {
        let start = if i == from.0 { from.1 } else { 0 };
        let end = if i == to.0 { to.1 } else { usize::MAX };
        out.push_str(&slice(&span.text, start, end));
    }
    out
}

fn find_chars(hay: &[char], needle: &[char], fold: bool) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let same = |a: &char, b: &char| if fold { a.to_lowercase().eq(b.to_lowercase()) } else { a == b };
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()].iter().zip(needle).all(|(a, b)| same(a, b)))
}

/// `needle`, with its whitespace squeezed, among the chapter's spans, which
/// are read with their whitespace squeezed the same way. Exact first, then
/// regardless of case.
fn find_in_chapter(html: &str, needle: &str) -> Option<Found> {
    let spans = spans_of(html);
    // Every character of the squeezed text knows its span and where it is in
    // it, counted the way the device counts (UTF-16 units).
    let mut squeezed: Vec<char> = Vec::new();
    let mut origin: Vec<(usize, usize, usize)> = Vec::new();
    let mut span_starts: Vec<usize> = Vec::with_capacity(spans.len());
    let mut total = 0usize;
    let mut pending_space = false;
    for (si, span) in spans.iter().enumerate() {
        span_starts.push(total);
        let mut at = 0usize;
        for c in span.text.chars() {
            let width = c.len_utf16();
            if c.is_whitespace() {
                pending_space = true;
            } else {
                if pending_space && !squeezed.is_empty() {
                    squeezed.push(' ');
                    origin.push((si, at, at));
                }
                pending_space = false;
                squeezed.push(c);
                origin.push((si, at, at + width));
            }
            at += width;
        }
        total += at;
    }
    let needle: Vec<char> = needle.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let start = find_chars(&squeezed, &needle, false).or_else(|| find_chars(&squeezed, &needle, true))?;
    let (first_span, first_at, _) = origin[start];
    let (last_span, _, last_end) = origin[start + needle.len() - 1];
    let progress = if total == 0 { 0.0 } else { (span_starts[first_span] + first_at) as f64 / total as f64 };
    Some(Found {
        start_path: selector(&spans[first_span].id),
        start_char: first_at,
        end_path: selector(&spans[last_span].id),
        end_char: last_end,
        progress,
        text: text_between(&spans, (first_span, first_at), (last_span, last_end)),
    })
}

/// A small KEPUB as kepubify makes one: a cover page and a chapter whose
/// sentences are wrapped in kobo spans.
#[cfg(test)]
pub fn sample_kepub() -> Vec<u8> {
    use std::io::Write;
    let chapter = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter X: The Lobster Quadrille</title></head><body>
<p><span class="koboSpan" id="kobo.4.1">
“No, indeed,” said Alice. </span><span class="koboSpan" id="kobo.4.2">“What sort of a dance is it?”
</span></p>
<p><span class="koboSpan" id="kobo.5.1">
“Why,” said the Gryphon, “you first form into a line along the sea-shore&mdash;”
</span></p>
<p><span class="koboSpan" id="kobo.31.1">
“You’re wrong about the crumbs,” said the Mock Turtle: “crumbs would all wash
off in the sea. </span><span class="koboSpan" id="kobo.31.2">But they </span><i><span class="koboSpan" id="kobo.31.3">have</span></i><span class="koboSpan" id="kobo.31.4"> their tails in their mouths; and the
reason is—” here the Mock Turtle yawned and shut his eyes.
</span></p>
</body></html>"#;
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">sample</dc:identifier><dc:title>Sample</dc:title><dc:language>en</dc:language></metadata>
<manifest>
<item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/>
<item id="ch1" href="xhtml/ch1.xhtml" media-type="application/xhtml+xml"/>
</manifest>
<spine><itemref idref="cover"/><itemref idref="ch1"/></spine>
</package>"#;
    let container = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("OEBPS/content.opf", opf),
        ("OEBPS/cover.xhtml", "<html><body><p>Cover</p></body></html>"),
        ("OEBPS/xhtml/ch1.xhtml", chapter),
    ] {
        zip.start_file(name, stored).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_at(text: &str, needle: &str) -> usize {
        text[..text.find(needle).unwrap()].encode_utf16().count()
    }

    #[test]
    fn the_spine_item_comes_out_of_the_cfi() {
        assert_eq!(spine_item_of("epubcfi(/6/4[ch1]!/4/2,/1:0,/1:5)"), Some((Some("ch1".into()), Some(1))));
        assert_eq!(spine_item_of("epubcfi(/6/20!/4/12,/1:0,/1:6)"), Some((None, Some(9))));
        assert_eq!(spine_item_of("epubcfi(/6/2[cover]!/4/2/1:0)"), Some((Some("cover".into()), Some(0))));
        assert_eq!(spine_item_of("nonsense"), None);
    }

    #[test]
    fn a_highlight_within_one_sentence_is_placed_by_its_span_and_offsets() {
        let kepub = sample_kepub();
        let placed = place(&kepub, "epubcfi(/6/4[ch1]!/4/2,/1:15,/1:25)", "said Alice").unwrap().unwrap();
        assert_eq!(placed.chapter, "OEBPS/xhtml/ch1.xhtml");
        assert_eq!(placed.title.as_deref(), Some("Chapter X: The Lobster Quadrille"));
        assert_eq!(placed.start_path, "span#kobo\\.4\\.1");
        assert_eq!(placed.end_path, "span#kobo\\.4\\.1");
        let span = "\n“No, indeed,” said Alice. ";
        assert_eq!(placed.start_char, utf16_at(span, "said Alice"));
        assert_eq!(placed.end_char, utf16_at(span, "said Alice") + "said Alice".encode_utf16().count());
        assert_eq!(placed.text, "said Alice");
        assert!(placed.progress > 0.0 && placed.progress < 0.1, "{}", placed.progress);
        assert_eq!(placed.location()["span"]["startPath"], "span#kobo\\.4\\.1");
        assert_eq!(placed.location()["span"]["chapterFilename"], "OEBPS/xhtml/ch1.xhtml");
    }

    #[test]
    fn a_highlight_across_sentences_and_through_markup_runs_from_span_to_span() {
        let kepub = sample_kepub();
        let placed = place(&kepub, "epubcfi(/6/4[ch1]!/4/2,/1:20,/3:10)", "Alice. “What sort").unwrap().unwrap();
        assert_eq!((placed.start_path.as_str(), placed.end_path.as_str()), ("span#kobo\\.4\\.1", "span#kobo\\.4\\.2"));
        assert_eq!(placed.end_char, "“What sort".encode_utf16().count());
        assert_eq!(placed.text, "Alice. “What sort");

        // The web reader squeezes whitespace; the chapter has line breaks and an entity.
        let placed = place(&kepub, "epubcfi(/6/4[ch1]!/4/6,/1:0,/5:10)", "But they have their tails in their mouths; and the reason is—”").unwrap().unwrap();
        assert_eq!((placed.start_path.as_str(), placed.end_path.as_str()), ("span#kobo\\.31\\.2", "span#kobo\\.31\\.4"));
        assert_eq!(placed.start_char, 0);
        assert_eq!(placed.text, "But they have their tails in their mouths; and the\nreason is—”");
        let placed = place(&kepub, "epubcfi(/6/4[ch1]!/4/4,/1:0,/1:5)", "along the sea-shore—”").unwrap().unwrap();
        assert_eq!(placed.start_path, "span#kobo\\.5\\.1");
    }

    #[test]
    fn the_chapter_comes_from_the_position_when_the_cfi_names_no_id_and_case_is_forgiven() {
        let kepub = sample_kepub();
        let placed = place(&kepub, "epubcfi(/6/4!/4/2,/1:0,/1:5)", "SAID ALICE").unwrap().unwrap();
        assert_eq!(placed.text, "said Alice");
        assert!(place(&kepub, "epubcfi(/6/4[ch1]!/4/2,/1:0,/1:5)", "not in the book").unwrap().is_none());
        assert!(place(&kepub, "epubcfi(/6/2[cover]!/4/2,/1:0,/1:5)", "Cover").unwrap().is_none(), "no spans on the cover page");
        assert!(place(&kepub, "epubcfi(/6/8!/4/2,/1:0,/1:5)", "said Alice").unwrap().is_none(), "no such spine item");
    }
}
