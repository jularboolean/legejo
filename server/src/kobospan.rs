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
    /// The id of the span it starts in, `kobo.<paragraph>.<sentence>`.
    pub start_id: String,
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
    placed(kepub, cfi, text, false)
}

/// As `place`, for a reader's place rather than a passage: where the words
/// do not all agree, a stretch of them that does will do.
pub fn place_near(kepub: &[u8], cfi: &str, text: &str) -> anyhow::Result<Option<Placed>> {
    placed(kepub, cfi, text, true)
}

fn placed(kepub: &[u8], cfi: &str, text: &str, loose: bool) -> anyhow::Result<Option<Placed>> {
    let Some((path, html)) = chapter_of(kepub, cfi)? else { return Ok(None) };
    Ok(find_in_chapter(&html, text, loose).map(|found| Placed {
        chapter: path,
        title: title_of(&html),
        start_id: found.start_id,
        start_path: found.start_path,
        start_char: found.start_char,
        end_path: found.end_path,
        end_char: found.end_char,
        progress: found.progress,
        text: found.text,
    }))
}

/// The file and text of the chapter the CFI names, in this KEPUB.
fn chapter_of(kepub: &[u8], cfi: &str) -> anyhow::Result<Option<(String, String)>> {
    let Some((idref, index)) = spine_item_of(cfi) else { return Ok(None) };
    let mut doc = EpubDoc::from_reader(Cursor::new(kepub)).map_err(|e| anyhow::anyhow!("could not read the kepub: {e}"))?;
    let item = idref
        .and_then(|id| doc.spine.iter().find(|s| s.idref == id).cloned())
        .or_else(|| index.and_then(|i| doc.spine.get(i).cloned()));
    let Some(item) = item else { return Ok(None) };
    let Some(path) = doc.resources.get(&item.idref).map(|r| r.path.to_string_lossy().replace('\\', "/")) else { return Ok(None) };
    let Some((html, _)) = doc.get_resource_str(&item.idref) else { return Ok(None) };
    Ok(Some((path, html)))
}

/// The first span of the chapter the CFI names: the right chapter at least,
/// for a place whose words cannot be found.
pub fn chapter_start(kepub: &[u8], cfi: &str) -> anyhow::Result<Option<Placed>> {
    let Some((path, html)) = chapter_of(kepub, cfi)? else { return Ok(None) };
    Ok(spans_of(&html).into_iter().next().map(|first| Placed {
        chapter: path,
        title: title_of(&html),
        start_id: first.id,
        start_path: String::new(),
        start_char: 0,
        end_path: String::new(),
        end_char: 0,
        progress: 0.0,
        text: first.text,
    }))
}

/// The words at a place the device names, by its span and chapter file:
/// the span's text and what follows it in the same paragraph, up to about
/// `PLACE_TEXT_CHARS`, whitespace squeezed. The web reader looks for them to
/// open the book at that line. A paragraph's spans share the `kobo.<n>.`
/// prefix; the text stops at the paragraph's end, where kepubify and the web
/// reader's copy may break lines differently. None when the chapter or the
/// span is not in this KEPUB.
pub fn text_at(kepub: &[u8], chapter: &str, span_id: &str) -> anyhow::Result<Option<String>> {
    let mut doc = EpubDoc::from_reader(Cursor::new(kepub)).map_err(|e| anyhow::anyhow!("could not read the kepub: {e}"))?;
    let wanted = chapter.trim_start_matches('/');
    let id = doc
        .resources
        .iter()
        .find(|(_, r)| {
            let path = r.path.to_string_lossy().replace('\\', "/");
            path == wanted || path.ends_with(&format!("/{wanted}")) || wanted.ends_with(&format!("/{path}"))
        })
        .map(|(id, _)| id.clone());
    let Some(id) = id else { return Ok(None) };
    let Some((html, _)) = doc.get_resource_str(&id) else { return Ok(None) };
    let spans = spans_of(&html);
    let Some(start) = spans.iter().position(|s| s.id == span_id) else { return Ok(None) };
    let paragraph = span_id.rsplit_once('.').map(|(p, _)| format!("{p}.")).unwrap_or_default();
    let mut text = String::new();
    for span in spans[start..].iter().take_while(|s| s.id.starts_with(&paragraph)) {
        text.push(' ');
        text.push_str(&span.text);
        if text.chars().count() >= PLACE_TEXT_CHARS {
            break;
        }
    }
    let squeezed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = squeezed.chars().take(PLACE_TEXT_CHARS).collect();
    // Not in the middle of a word: a cut word would not be found.
    let cut = match (cut.chars().count() < squeezed.chars().count(), cut.rfind(' ')) {
        (true, Some(space)) if space > 0 => cut[..space].to_string(),
        _ => cut,
    };
    Ok((!cut.is_empty()).then_some(cut))
}

/// About how much of a paragraph `text_at` hands back.
const PLACE_TEXT_CHARS: usize = 120;

struct Span {
    id: String,
    text: String,
}

struct Found {
    start_id: String,
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

/// How many letters and digits of the words make a place, when they do not
/// all agree: enough not to be found by chance in one chapter.
const LOOSE_WINDOW: usize = 30;

/// Where the needle's letters and digits run in the chapter's, whatever lies
/// between them; or, failing that, the first stretch of them that does. The
/// web reader's text can run on past what the KEPUB holds, so a few words
/// that agree beat a place not found. A start and a length in `solid`.
fn find_loose(solid: &[char], needle: &[char]) -> Option<(usize, usize)> {
    let squeezed = |text: &[char]| -> Vec<(usize, String)> {
        text.iter().enumerate().filter(|(_, c)| c.is_alphanumeric()).map(|(i, c)| (i, c.to_lowercase().collect())).collect()
    };
    let hay = squeezed(solid);
    let wanted = squeezed(needle);
    let at = |from: usize, len: usize| -> Option<(usize, usize)> {
        let part = &wanted[from..from + len];
        let hit = (0..=hay.len().checked_sub(len)?).find(|&i| hay[i..i + len].iter().zip(part).all(|(a, b)| a.1 == b.1))?;
        Some((hay[hit].0, hay[hit + len - 1].0 + 1 - hay[hit].0))
    };
    if wanted.is_empty() {
        return None;
    }
    at(0, wanted.len()).or_else(|| {
        (0..wanted.len().saturating_sub(LOOSE_WINDOW - 1)).step_by(LOOSE_WINDOW / 2).find_map(|from| at(from, LOOSE_WINDOW))
    })
}

/// `needle` among the chapter's spans, both read without their whitespace:
/// kepubify breaks lines where the web reader's copy has none (after a
/// `<br/>`, between the cells of a table), and the web reader squeezes
/// what it sends. Exact first, then regardless of case, then by letters and
/// digits alone.
fn find_in_chapter(html: &str, needle: &str, loose: bool) -> Option<Found> {
    let spans = spans_of(html);
    // Every character of the solid text knows its span and where it is in
    // it, counted the way the device counts (UTF-16 units).
    let mut solid: Vec<char> = Vec::new();
    let mut origin: Vec<(usize, usize, usize)> = Vec::new();
    let mut span_starts: Vec<usize> = Vec::with_capacity(spans.len());
    let mut total = 0usize;
    for (si, span) in spans.iter().enumerate() {
        span_starts.push(total);
        let mut at = 0usize;
        for c in span.text.chars() {
            let width = c.len_utf16();
            if !c.is_whitespace() {
                solid.push(c);
                origin.push((si, at, at + width));
            }
            at += width;
        }
        total += at;
    }
    let needle: Vec<char> = needle.chars().filter(|c| !c.is_whitespace()).collect();
    let exact = find_chars(&solid, &needle, false).or_else(|| find_chars(&solid, &needle, true));
    let (start, length) = exact.map(|start| (start, needle.len())).or_else(|| find_loose(&solid, &needle).filter(|_| loose))?;
    let (first_span, first_at, _) = origin[start];
    let (last_span, _, last_end) = origin[start + length - 1];
    let progress = if total == 0 { 0.0 } else { (span_starts[first_span] + first_at) as f64 / total as f64 };
    Some(Found {
        start_id: spans[first_span].id.clone(),
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
    fn the_text_at_a_span_runs_to_the_end_of_its_paragraph() {
        let kepub = sample_kepub();
        let text = text_at(&kepub, "OEBPS/xhtml/ch1.xhtml", "kobo.4.1").unwrap().unwrap();
        assert_eq!(text, "“No, indeed,” said Alice. “What sort of a dance is it?”");
        // The chapter may be named without the archive's folder.
        assert_eq!(text_at(&kepub, "xhtml/ch1.xhtml", "kobo.5.1").unwrap().unwrap(), "“Why,” said the Gryphon, “you first form into a line along the sea-shore—”");
        // Markup inside the paragraph (the <i>) is dropped, lines squeezed, and a
        // long paragraph cut between words.
        let long = text_at(&kepub, "OEBPS/xhtml/ch1.xhtml", "kobo.31.2").unwrap().unwrap();
        assert!(long.starts_with("But they have their tails in their mouths; and the reason is"), "{long}");
        assert!(long.chars().count() <= PLACE_TEXT_CHARS);
        assert!(!long.contains('\n'));
        // Unknown spans and chapters.
        assert_eq!(text_at(&kepub, "OEBPS/xhtml/ch1.xhtml", "kobo.99.1").unwrap(), None);
        assert_eq!(text_at(&kepub, "OEBPS/xhtml/nope.xhtml", "kobo.4.1").unwrap(), None);
    }

    #[test]
    fn a_place_is_found_by_its_letters_when_the_rest_does_not_agree() {
        let kepub = sample_kepub();
        let cfi = "epubcfi(/6/4[ch1]!/4/4/1:0)";
        // Other punctuation than the chapter's.
        let found = place_near(&kepub, cfi, "Why, said the Gryphon: you first form into a line along the sea shore").unwrap().unwrap();
        assert_eq!(found.start_id, "kobo.5.1");
        // Words the chapter does not have before the ones it has.
        let found = place_near(&kepub, cfi, "something quite else that no chapter holds at all, You’re wrong about the crumbs, said the Mock Turtle: crumbs would all wash off in the sea").unwrap().unwrap();
        assert_eq!(found.start_id, "kobo.31.1");
        // A passage to mark is exact, or not placed; nor is anything found in nothing like it.
        assert!(place(&kepub, cfi, "Why, said the Gryphon: you first form").unwrap().is_none());
        assert!(place_near(&kepub, cfi, "words that are in no chapter of this book whatsoever, none of them").unwrap().is_none());
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
        assert_eq!(placed.start_id, "kobo.4.1");
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

        // The web reader's copy runs the sentences together where kepubify
        // broke the line; the device still gets the place.
        let placed = place(&kepub, "epubcfi(/6/4[ch1]!/4/2,/1:20,/3:10)", "said Alice.“What sort of a dance").unwrap().unwrap();
        assert_eq!((placed.start_path.as_str(), placed.end_path.as_str()), ("span#kobo\\.4\\.1", "span#kobo\\.4\\.2"));
        assert_eq!(placed.text, "said Alice. “What sort of a dance");
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
