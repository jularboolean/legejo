//! A PDF with the reader's highlights and notes written into it, as the
//! annotations every PDF reader knows, for a copy to take elsewhere. The file
//! in the library stays as it is: it is the one the device knows by its hash.

use crate::auth::AuthUser;
use crate::books::{book_file, download_name, owned_book};
use crate::formats::Format;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use serde_json::json;

/// The highlight's yellow in the reader, for one with no colour of its own.
const YELLOW: [f32; 3] = [0.949, 0.788, 0.298];
const INK: f32 = 0.45;

/// A rectangle on a page as the reader keeps it: fractions of the page as it
/// is shown, from the top left.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    top: f32,
    left: f32,
    height: f32,
    width: f32,
}

/// One highlight, as it goes into the file.
pub struct Mark {
    pages: Vec<(u32, Vec<Rect>)>,
    note: Option<String>,
    color: [f32; 3],
    author: String,
    at: String,
}

/// The passage in a `pdf:` place (web/src/lib/reader/pdf/anchor.ts). Nothing
/// for a bare position, or anything else.
fn passage(cfi: &str) -> Option<Vec<(u32, Vec<Rect>)>> {
    let mut pages = Vec::new();
    for part in cfi.strip_prefix("pdf:")?.split('|') {
        let (page, rects) = part.split_once(':')?;
        let rects = rects
            .split('/')
            .map(|r| {
                let n: Vec<f32> = r.split(',').map(|x| x.parse().ok()).collect::<Option<_>>()?;
                (n.len() == 4).then(|| Rect { top: n[0], left: n[1], height: n[2], width: n[3] })
            })
            .collect::<Option<Vec<_>>>()?;
        pages.push((page.parse().ok()?, rects));
    }
    Some(pages)
}

fn color_of(hex: Option<&str>) -> [f32; 3] {
    let channels = hex.and_then(|h| {
        let h = h.strip_prefix('#')?;
        let channel = |at: usize| u8::from_str_radix(h.get(at..at + 2)?, 16).ok();
        Some([channel(0)?, channel(2)?, channel(4)?])
    });
    channels.map_or(YELLOW, |c| c.map(|v| f32::from(v) / 255.0))
}

/// The page's own value of a key, or the one it inherits.
fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut at = doc.get_dictionary(page).ok()?;
    for _ in 0..64 {
        if let Ok(value) = at.get(key) {
            return doc.dereference(value).ok().map(|(_, o)| o.clone());
        }
        at = doc.get_dictionary(at.get(b"Parent").ok()?.as_reference().ok()?).ok()?;
    }
    None
}

/// The page's box as left, bottom, right, top.
fn page_box(doc: &Document, page: ObjectId) -> Option<[f32; 4]> {
    let boxed = inherited(doc, page, b"CropBox").or_else(|| inherited(doc, page, b"MediaBox"))?;
    let numbers: Vec<f32> = boxed
        .as_array()
        .ok()?
        .iter()
        .map(|n| doc.dereference(n).ok().and_then(|(_, n)| n.as_float().ok()))
        .collect::<Option<_>>()?;
    let [a, b, c, d] = numbers[..] else { return None };
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

/// A point given as fractions of the page as shown, in the page's own space.
fn on_page(area: [f32; 4], rotation: i64, x: f32, y: f32) -> (f32, f32) {
    let [left, bottom, right, top] = area;
    let (width, height) = (right - left, top - bottom);
    match rotation.rem_euclid(360) {
        90 => (left + y * width, bottom + x * height),
        180 => (right - x * width, bottom + y * height),
        270 => (right - y * width, top - x * height),
        _ => (left + x * width, top - y * height),
    }
}

/// A rectangle's four corners, as `QuadPoints` wants them: top left, top
/// right, bottom left, bottom right, as shown.
fn corners(area: [f32; 4], rotation: i64, r: Rect) -> [(f32, f32); 4] {
    let point = |x, y| on_page(area, rotation, x, y);
    let (right, bottom) = (r.left + r.width, r.top + r.height);
    [point(r.left, r.top), point(right, r.top), point(r.left, bottom), point(right, bottom)]
}

fn text_string(text: &str) -> Object {
    let bytes = [0xFE, 0xFF].into_iter().chain(text.encode_utf16().flat_map(u16::to_be_bytes)).collect();
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// `2026-11-01T12:30:00.000Z` as a PDF date, or nothing.
fn pdf_date(at: &str) -> Option<String> {
    let digits: String = at.chars().take(19).filter(char::is_ascii_digit).collect();
    (digits.len() == 14).then(|| format!("D:{digits}Z"))
}

fn reals(numbers: &[f32]) -> Object {
    Object::Array(numbers.iter().map(|n| Object::Real(*n)).collect())
}

/// The highlight as the page draws it: not all readers paint one themselves.
fn appearance(doc: &mut Document, bounds: [f32; 4], quads: &[[(f32, f32); 4]], color: [f32; 3]) -> ObjectId {
    let mut content = format!("/Ink gs {} {} {} rg\n", color[0], color[1], color[2]);
    for [a, b, c, d] in quads {
        // Around the rectangle: the corners are given line by line.
        content += &format!("{} {} m {} {} l {} {} l {} {} l h f\n", a.0, a.1, b.0, b.1, d.0, d.1, c.0, c.1);
    }
    let ink = doc.add_object(dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "ca" => Object::Real(INK) });
    doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => reals(&bounds),
            "Group" => dictionary! { "S" => "Transparency" },
            "Resources" => dictionary! { "ExtGState" => dictionary! { "Ink" => ink } },
        },
        content.into_bytes(),
    ))
}

fn annotate_page(doc: &mut Document, page: ObjectId, rects: &[Rect], mark: &Mark, note: Option<&str>) -> Option<()> {
    let area = page_box(doc, page)?;
    let rotation = inherited(doc, page, b"Rotate").and_then(|r| r.as_i64().ok()).unwrap_or(0);
    let quads: Vec<[(f32, f32); 4]> = rects.iter().map(|r| corners(area, rotation, *r)).collect();
    let points = || quads.iter().flatten();
    let bounds = [
        points().map(|p| p.0).fold(f32::MAX, f32::min),
        points().map(|p| p.1).fold(f32::MAX, f32::min),
        points().map(|p| p.0).fold(f32::MIN, f32::max),
        points().map(|p| p.1).fold(f32::MIN, f32::max),
    ];
    let flat: Vec<f32> = points().flat_map(|p| [p.0, p.1]).collect();
    let drawn = appearance(doc, bounds, &quads, mark.color);
    let mut annotation = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Highlight",
        "Rect" => reals(&bounds),
        "QuadPoints" => reals(&flat),
        "C" => reals(&mark.color),
        "CA" => Object::Real(INK),
        // Printed with the page.
        "F" => 4,
        "P" => page,
        "T" => text_string(&mark.author),
        "AP" => dictionary! { "N" => drawn },
    };
    if let Some(note) = note {
        annotation.set("Contents", text_string(note));
    }
    if let Some(date) = pdf_date(&mark.at) {
        annotation.set("M", Object::string_literal(date));
    }
    let id = doc.add_object(annotation);
    append_annotation(doc, page, id)
}

/// Onto the page's list, which may be its own object or lie in the page.
fn append_annotation(doc: &mut Document, page: ObjectId, id: ObjectId) -> Option<()> {
    let existing = doc.get_dictionary(page).ok()?.get(b"Annots").ok().cloned();
    if let Some(Object::Reference(list)) = existing {
        doc.get_object_mut(list).ok()?.as_array_mut().ok()?.push(id.into());
        return Some(());
    }
    let mut list = existing.and_then(|l| l.as_array().ok().cloned()).unwrap_or_default();
    list.push(id.into());
    doc.get_dictionary_mut(page).ok()?.set("Annots", list);
    Some(())
}

/// A copy of the PDF with the marks in it, and how many went in.
pub fn with_marks(pdf: &[u8], marks: &[Mark]) -> anyhow::Result<(Vec<u8>, usize)> {
    let mut doc = Document::load_mem(pdf)?;
    let pages = doc.get_pages();
    let mut written = 0;
    for mark in marks {
        // The note once, on the page where the passage begins.
        let mut note = mark.note.as_deref();
        let mut placed = false;
        for (number, rects) in &mark.pages {
            let Some(page) = pages.get(number) else { continue };
            if annotate_page(&mut doc, *page, rects, mark, note).is_some() {
                note = None;
                placed = true;
            }
        }
        written += usize::from(placed);
    }
    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok((out, written))
}

/// GET /api/books/{id}/annotated: the PDF with the user's own highlights and
/// notes in it, for download. The file stays as it is.
pub async fn annotated(State(state): State<AppState>, user: AuthUser, Path(id): Path<i64>) -> Result<Response, Response> {
    let refused = |status: StatusCode, code: &str| (status, Json(json!({ "error": code }))).into_response();
    let (uuid, _, title) = owned_book(&state, user.0.id, id).await?;
    let (path, format) = book_file(&state, &uuid).await;
    if !matches!(format, Format::Pdf) {
        return Err(refused(StatusCode::NOT_FOUND, "not a pdf"));
    }
    let rows: Vec<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT cfi, note, color, updated_at FROM annotations
         WHERE book_id = $1 AND user_id = $2 AND cfi LIKE 'pdf:%' ORDER BY id",
    )
    .bind(id)
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("internal error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;
    let marks: Vec<Mark> = rows
        .into_iter()
        .filter_map(|(cfi, note, color, at)| {
            let pages = passage(&cfi)?;
            Some(Mark { pages, note, color: color_of(color.as_deref()), author: user.0.username.clone(), at })
        })
        .collect();
    let pdf = tokio::fs::read(&path).await.map_err(|e| {
        tracing::error!("reading {}: {e}", path.display());
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })?;
    let done = tokio::task::spawn_blocking(move || with_marks(&pdf, &marks)).await;
    let (bytes, written) = match done {
        Ok(Ok(done)) => done,
        Ok(Err(e)) => {
            tracing::warn!("marking up book {id}: {e:#}");
            return Err(refused(StatusCode::UNPROCESSABLE_ENTITY, "pdf not understood"));
        }
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    };
    tracing::info!("book {id}: a PDF with {written} highlights made");
    let name = download_name(&title, Format::Pdf).replace(".pdf", " - annotated.pdf");
    Ok((
        [
            (header::CONTENT_TYPE, Format::Pdf.mime().to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF of two pages, the second turned a quarter, in a box that does
    /// not begin at the origin.
    fn sample() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages = doc.new_object_id();
        let plain = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages });
        let list = doc.add_object(Object::Array(vec![]));
        let turned = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Rotate" => 90, "Annots" => list,
            "CropBox" => vec![10.into(), 20.into(), 310.into(), 420.into()],
        });
        doc.objects.insert(pages, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![plain.into(), turned.into()], "Count" => 2,
            "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn mark(cfi: &str, note: Option<&str>) -> Mark {
        Mark { pages: passage(cfi).unwrap(), note: note.map(str::to_string), color: YELLOW, author: "Åsa".into(), at: "2026-11-01T12:30:05.000Z".into() }
    }

    fn annotations(doc: &Document, page: u32) -> Vec<Dictionary> {
        let id = doc.get_pages()[&page];
        let Ok(list) = doc.get_dictionary(id).unwrap().get(b"Annots") else { return vec![] };
        let (_, list) = doc.dereference(list).unwrap();
        list.as_array().unwrap().iter().map(|a| doc.dereference(a).unwrap().1.as_dict().unwrap().clone()).collect()
    }

    fn numbers(object: &Object) -> Vec<f32> {
        object.as_array().unwrap().iter().map(|n| n.as_float().unwrap()).collect()
    }

    #[test]
    fn a_place_is_read_as_the_reader_wrote_it() {
        assert_eq!(passage("pdf:12:0.1,0.2,0.03,0.5/0.2,0.2,0.03,0.4|13:0.05,0.1,0.02,0.5").unwrap().len(), 2);
        assert!(passage("pdf:12").is_none(), "a position, no passage");
        assert!(passage("epubcfi(/6/4!/4/2/1:0)").is_none());
        assert!(passage("pdf:2:1,2,3").is_none());
        assert_eq!(color_of(Some("#ff0000")), [1.0, 0.0, 0.0]);
        assert_eq!(color_of(Some("yellow")), YELLOW);
    }

    #[test]
    fn a_highlight_goes_in_where_the_reader_saw_it() {
        let marks = [mark("pdf:1:0.25,0.5,0.1,0.25", Some("Bra sagt, åäö")), mark("pdf:2:0.25,0.5,0.1,0.25", None), mark("pdf:9:0,0,0.1,0.1", None)];
        let (bytes, written) = with_marks(&sample(), &marks).unwrap();
        assert_eq!(written, 2, "a page the file lacks is left out");
        let doc = Document::load_mem(&bytes).unwrap();

        // Upright: from the top left of the inherited box of 600 by 800.
        let first = &annotations(&doc, 1)[0];
        assert_eq!(first.get(b"Subtype").unwrap().as_name().unwrap(), b"Highlight");
        assert_eq!(numbers(first.get(b"Rect").unwrap()), [300.0, 520.0, 450.0, 600.0]);
        assert_eq!(numbers(first.get(b"QuadPoints").unwrap()), [300.0, 600.0, 450.0, 600.0, 300.0, 520.0, 450.0, 520.0]);
        let (Object::String(note, _), Object::String(author, _)) = (first.get(b"Contents").unwrap(), first.get(b"T").unwrap()) else { panic!() };
        let utf16 = |b: &[u8]| String::from_utf16(&b[2..].chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap();
        assert_eq!((utf16(note), utf16(author)), ("Bra sagt, åäö".to_string(), "Åsa".to_string()));
        assert!(first.get(b"AP").is_ok(), "drawn, for the readers that do not paint one");

        // Turned a quarter, in its crop box of 300 by 400 at (10, 20).
        let turned = &annotations(&doc, 2)[0];
        assert_eq!(numbers(turned.get(b"Rect").unwrap()), [85.0, 220.0, 115.0, 320.0]);
        assert!(turned.get(b"Contents").is_err());
    }

    #[test]
    fn a_note_is_written_once_for_a_passage_over_pages() {
        let (bytes, _) = with_marks(&sample(), &[mark("pdf:1:0.1,0.1,0.1,0.1|2:0.1,0.1,0.1,0.1", Some("en"))]).unwrap();
        let doc = Document::load_mem(&bytes).unwrap();
        assert!(annotations(&doc, 1)[0].get(b"Contents").is_ok());
        assert!(annotations(&doc, 2)[0].get(b"Contents").is_err());
    }
}
