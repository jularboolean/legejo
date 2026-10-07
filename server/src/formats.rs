//! The file formats a book can have. EPUB is the one Legejo reads, tends and
//! converts; PDF and CBZ are stored, described and handed to devices as they
//! are.

use std::io::{Cursor, Read, Seek, Write};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Epub,
    Pdf,
    Cbz,
}

impl Format {
    /// The name stored in `books.format`, which is also the file extension.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Pdf => "pdf",
            Format::Cbz => "cbz",
        }
    }

    /// Unknown names count as EPUB, the format of every book before others
    /// existed.
    pub fn parse(name: &str) -> Format {
        match name {
            "pdf" => Format::Pdf,
            "cbz" => Format::Cbz,
            _ => Format::Epub,
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Epub => "application/epub+zip",
            Format::Pdf => "application/pdf",
            Format::Cbz => "application/vnd.comicbook+zip",
        }
    }

    /// What an uploaded file is: by its first bytes, and for the two zip
    /// formats by its name. None for anything else.
    pub fn detect(bytes: &[u8], filename: &str) -> Option<Format> {
        let name = filename.to_lowercase();
        if bytes.starts_with(b"%PDF-") {
            return Some(Format::Pdf);
        }
        if !bytes.starts_with(b"PK") {
            return None;
        }
        if name.ends_with(".cbz") {
            Some(Format::Cbz)
        } else if name.ends_with(".pdf") {
            None
        } else {
            Some(Format::Epub)
        }
    }
}

/// `name` without the extension of a book file.
pub fn stem(name: &str) -> &str {
    let lower = name.to_lowercase();
    for ext in [".epub", ".pdf", ".cbz"] {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// What a PDF or a CBZ says about itself.
#[derive(Default, Debug)]
pub struct Found {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub published: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    /// The image and its media type.
    pub cover: Option<(Vec<u8>, String)>,
    pub subjects: Vec<String>,
}

fn clean(value: String) -> Option<String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!value.is_empty()).then_some(value)
}

/// A text string of a PDF: UTF-16 or UTF-8 when it starts with the mark of
/// one, otherwise one byte per character.
fn pdf_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|b| *b as char).collect()
    }
}

/// Titles that programs put there on their own and that say nothing.
fn empty_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower == "untitled"
        || lower.starts_with("microsoft word - ")
        || [".doc", ".docx", ".indd", ".qxd", ".dvi", ".tex", ".pdf"].iter().any(|ext| lower.ends_with(ext))
}

/// Title and author from the document information, and as the cover the
/// first page when that page is a single JPEG, as in a scanned book.
/// Err("copy-protected") for a file that needs a password; a file that does
/// not read otherwise is kept, with nothing found in it.
pub fn inspect_pdf(bytes: &[u8]) -> Result<Found, &'static str> {
    let doc = match lopdf::Document::load_mem(bytes) {
        Ok(doc) => doc,
        Err(e) => {
            let text = e.to_string().to_lowercase();
            if text.contains("decrypt") || text.contains("password") || text.contains("encrypt") {
                return Err("copy-protected");
            }
            tracing::debug!("pdf not read: {e}");
            return Ok(Found::default());
        }
    };
    if doc.is_encrypted() {
        return Err("copy-protected");
    }
    let mut found = Found::default();
    if let Ok(info) = doc.trailer.get(b"Info").and_then(|o| doc.dereference(o)).and_then(|(_, o)| o.as_dict()) {
        let text = |key: &[u8]| info.get(key).and_then(|o| o.as_str()).ok().map(pdf_text).and_then(clean);
        found.title = text(b"Title").filter(|t| !empty_title(t));
        found.author = text(b"Author");
        found.subjects = text(b"Keywords")
            .map(|k| k.split([',', ';']).filter_map(|t| clean(t.to_string())).filter(|t| t.len() <= 100).take(20).collect())
            .unwrap_or_default();
    }
    if let Some(page) = doc.get_pages().values().next() {
        if let Ok(images) = doc.get_page_images(*page) {
            if let [image] = images.as_slice() {
                let jpeg = image.filters.as_ref().is_some_and(|f| f.len() == 1 && f[0] == "DCTDecode");
                if jpeg && image.width >= 200 && image.height >= 200 {
                    found.cover = Some((image.content.to_vec(), "image/jpeg".to_string()));
                }
            }
        }
    }
    Ok(found)
}

fn image_type(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".webp") {
        Some("image/webp")
    } else if lower.ends_with(".gif") {
        Some("image/gif")
    } else {
        None
    }
}

/// Order names the way their pages are read: runs of digits by their value,
/// so that "page2" comes before "page10".
fn natural_key(name: &str) -> Vec<(u64, String)> {
    let mut key = Vec::new();
    let mut chars = name.to_lowercase().chars().collect::<Vec<_>>().into_iter().peekable();
    while chars.peek().is_some() {
        let mut digits = String::new();
        while let Some(c) = chars.peek().filter(|c| c.is_ascii_digit()) {
            digits.push(*c);
            chars.next();
        }
        let mut text = String::new();
        while let Some(c) = chars.peek().filter(|c| !c.is_ascii_digit()) {
            text.push(*c);
            chars.next();
        }
        key.push((digits.parse().unwrap_or(0), text));
    }
    key
}

/// The text of the first `<name>…</name>` in a ComicInfo.xml.
fn xml_field(xml: &str, name: &str) -> Option<String> {
    let start = xml.find(&format!("<{name}>"))? + name.len() + 2;
    let end = start + xml[start..].find(&format!("</{name}>"))?;
    let text = xml[start..end]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    clean(text)
}

/// The largest cover a comic archive is read for; pages are rarely near it.
const MAX_COVER_BYTES: u64 = 30 * 1024 * 1024;

/// The first page as the cover, and what a ComicInfo.xml tells. Err when the
/// archive does not open or holds no pages.
/// The pages of a comic archive in reading order, each with its media type,
/// and the name of its ComicInfo.xml when it has one.
fn comic_contents<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> (Vec<(String, &'static str)>, Option<String>) {
    let mut pages: Vec<(String, &'static str)> = Vec::new();
    let mut info: Option<String> = None;
    for i in 0..archive.len() {
        let Ok(file) = archive.by_index(i) else { continue };
        let name = file.name().to_string();
        let base = name.rsplit('/').next().unwrap_or(&name);
        // Folders of thumbnails and the like that archivers on macOS add.
        if name.starts_with("__MACOSX/") || base.starts_with('.') {
            continue;
        }
        if base.eq_ignore_ascii_case("ComicInfo.xml") {
            info = Some(name);
        } else if let Some(mime) = image_type(base) {
            pages.push((name, mime));
        }
    }
    pages.sort_by_key(|(name, _)| natural_key(name));
    (pages, info)
}

pub fn inspect_cbz(bytes: &[u8]) -> Result<Found, &'static str> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "not a readable comic archive")?;
    let (pages, info) = comic_contents(&mut archive);
    if pages.is_empty() {
        return Err("no pages in the comic archive");
    }
    let mut found = Found::default();
    let (first, mime) = &pages[0];
    if let Ok(file) = archive.by_name(first) {
        if file.size() <= MAX_COVER_BYTES {
            let mut data = Vec::new();
            if file.take(MAX_COVER_BYTES).read_to_end(&mut data).is_ok() {
                found.cover = Some((data, mime.to_string()));
            }
        }
    }
    if let Some(name) = info {
        let mut xml = String::new();
        if archive.by_name(&name).is_ok_and(|f| f.take(1024 * 1024).read_to_string(&mut xml).is_ok()) {
            let series = xml_field(&xml, "Series");
            let number = xml_field(&xml, "Number");
            found.title = xml_field(&xml, "Title").or_else(|| match (&series, &number) {
                (Some(series), Some(number)) => Some(format!("{series} {number}")),
                _ => None,
            });
            found.series_index = number.as_deref().and_then(|n| n.parse().ok());
            found.series = series;
            found.author = xml_field(&xml, "Writer");
            found.description = xml_field(&xml, "Summary");
            found.publisher = xml_field(&xml, "Publisher");
            found.language = xml_field(&xml, "LanguageISO");
            found.published = xml_field(&xml, "Year").filter(|y| y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()));
            found.subjects = xml_field(&xml, "Genre")
                .map(|g| g.split(',').filter_map(|t| clean(t.to_string())).filter(|t| t.len() <= 100).take(20).collect())
                .unwrap_or_default();
        }
    }
    Ok(found)
}

/// What the book made from a comic says about itself.
pub struct ComicMeta {
    pub uuid: String,
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Make an EPUB with a fixed layout out of a comic archive: one page per
/// image, in reading order, each as large as its image. The images are
/// carried over as they are (WebP, which e-readers do not all show, as
/// JPEG). E-readers that open EPUB but not comic archives can then read it,
/// and keep their place in it. Returns the number of pages.
pub fn cbz_to_epub(src: &std::path::Path, dst: &std::path::Path, meta: &ComicMeta) -> anyhow::Result<usize> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(src)?)?;
    let (pages, info) = comic_contents(&mut archive);
    anyhow::ensure!(!pages.is_empty(), "no pages in the comic archive");
    // Manga is read from the right.
    let mut right_to_left = false;
    if let Some(name) = info {
        let mut xml = String::new();
        if archive.by_name(&name).is_ok_and(|f| f.take(1024 * 1024).read_to_string(&mut xml).is_ok()) {
            right_to_left = xml_field(&xml, "Manga").is_some_and(|m| m.eq_ignore_ascii_case("YesAndRightToLeft"));
        }
    }

    let mut out = zip::ZipWriter::new(std::fs::File::create(dst)?);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
    let packed = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    out.start_file("mimetype", stored)?;
    out.write_all(b"application/epub+zip")?;
    out.start_file("META-INF/container.xml", packed)?;
    out.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>
"#,
    )?;
    out.start_file("OEBPS/style.css", packed)?;
    out.write_all(b"@page { margin: 0; }\nhtml, body { margin: 0; padding: 0; }\nimg { display: block; margin: 0; padding: 0; }\n")?;

    // (id, image file, media type, width, height) per page.
    let mut items: Vec<(String, String, &'static str, u32, u32)> = Vec::new();
    for (name, mime) in &pages {
        let mut data = Vec::new();
        archive.by_name(name)?.read_to_end(&mut data)?;
        // A page that is not the image its name says is left out.
        let Ok(reader) = image::ImageReader::new(Cursor::new(&data)).with_guessed_format() else { continue };
        let (mut mime, mut ext) = (*mime, name.rsplit('.').next().unwrap_or("jpg").to_lowercase());
        let (width, height) = if mime == "image/webp" {
            let Ok(decoded) = reader.decode() else { continue };
            let mut jpeg = Vec::new();
            decoded.to_rgb8().write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)?;
            data = jpeg;
            (mime, ext) = ("image/jpeg", "jpg".to_string());
            (decoded.width(), decoded.height())
        } else {
            let Ok(size) = reader.into_dimensions() else { continue };
            size
        };
        let id = format!("p{:04}", items.len() + 1);
        let file = format!("{id}.{ext}");
        out.start_file(format!("OEBPS/images/{file}"), stored)?;
        out.write_all(&data)?;
        out.start_file(format!("OEBPS/{id}.xhtml"), packed)?;
        write!(
            out,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
<meta charset="utf-8"/>
<title>{n}</title>
<meta name="viewport" content="width={width}, height={height}"/>
<link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body><div><img src="images/{file}" width="{width}" height="{height}" alt=""/></div></body>
</html>
"#,
            n = items.len() + 1,
        )?;
        items.push((id, file, mime, width, height));
    }
    anyhow::ensure!(!items.is_empty(), "no readable pages in the comic archive");

    let (title, language) = (esc(&meta.title), esc(meta.language.as_deref().unwrap_or("en")));
    out.start_file("OEBPS/nav.xhtml", packed)?;
    write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><meta charset="utf-8"/><title>{title}</title></head>
<body><nav epub:type="toc"><ol><li><a href="p0001.xhtml">{title}</a></li></ol></nav></body>
</html>
"#
    )?;
    out.start_file("OEBPS/toc.ncx", packed)?;
    write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx version="2005-1" xmlns="http://www.daisy.org/z3986/2005/ncx/">
<head><meta name="dtb:uid" content="urn:uuid:{uuid}"/></head>
<docTitle><text>{title}</text></docTitle>
<navMap><navPoint id="n1" playOrder="1"><navLabel><text>{title}</text></navLabel><content src="p0001.xhtml"/></navPoint></navMap>
</ncx>
"#,
        uuid = meta.uuid,
    )?;
    let (first_width, first_height) = (items[0].3, items[0].4);
    let creator = meta.author.as_deref().map(|a| format!("    <dc:creator>{}</dc:creator>\n", esc(a))).unwrap_or_default();
    let mut manifest = String::new();
    let mut spine = String::new();
    for (i, (id, file, mime, _, _)) in items.iter().enumerate() {
        let cover = if i == 0 { " properties=\"cover-image\"" } else { "" };
        manifest.push_str(&format!(
            "    <item id=\"{id}\" href=\"{id}.xhtml\" media-type=\"application/xhtml+xml\"/>\n    \
             <item id=\"i{id}\" href=\"images/{file}\" media-type=\"{mime}\"{cover}/>\n"
        ));
        spine.push_str(&format!("    <itemref idref=\"{id}\"/>\n"));
    }
    out.start_file("OEBPS/content.opf", packed)?;
    write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id" prefix="rendition: http://www.idpf.org/vocab/rendition/#">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">urn:uuid:{uuid}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:language>{language}</dc:language>
{creator}    <meta property="dcterms:modified">2020-01-01T00:00:00Z</meta>
    <meta name="cover" content="ip0001"/>
    <meta property="rendition:layout">pre-paginated</meta>
    <meta property="rendition:orientation">auto</meta>
    <meta property="rendition:spread">landscape</meta>
    <meta name="original-resolution" content="{first_width}x{first_height}"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
{manifest}  </manifest>
  <spine toc="ncx" page-progression-direction="{direction}">
{spine}  </spine>
</package>
"#,
        uuid = meta.uuid,
        direction = if right_to_left { "rtl" } else { "ltr" },
    )?;
    out.finish()?;
    Ok(items.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            for (name, data) in files {
                zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn formats_are_told_apart() {
        assert_eq!(Format::detect(b"%PDF-1.7\n", "x.epub"), Some(Format::Pdf));
        assert_eq!(Format::detect(b"PK\x03\x04", "Comic.CBZ"), Some(Format::Cbz));
        assert_eq!(Format::detect(b"PK\x03\x04", "book.epub"), Some(Format::Epub));
        assert_eq!(Format::detect(b"PK\x03\x04", "upload"), Some(Format::Epub));
        assert_eq!(Format::detect(b"PK\x03\x04", "odd.pdf"), None);
        assert_eq!(Format::detect(b"Rar!\x1a\x07", "comic.cbr"), None);
        assert_eq!(stem("Tintin 01.CBZ"), "Tintin 01");
        assert_eq!(stem("notes"), "notes");
        assert_eq!(Format::parse("pdf").as_str(), "pdf");
        assert_eq!(Format::parse("").as_str(), "epub");
    }

    #[test]
    fn a_comic_archive_gives_its_first_page_and_its_info() {
        let info = b"<?xml version=\"1.0\"?><ComicInfo><Series>Tintin &amp; Co</Series><Number>3</Number>\
            <Writer>Herg\xc3\xa9</Writer><Year>1946</Year><LanguageISO>fr</LanguageISO><Genre>Adventure, Humour</Genre></ComicInfo>";
        let cbz = zip_of(&[
            ("pages/page10.jpg", b"ten"),
            ("pages/page2.jpg", b"two"),
            ("__MACOSX/pages/._page1.jpg", b"junk"),
            ("ComicInfo.xml", info),
            ("pages/Page1.PNG", b"one"),
        ]);
        let found = inspect_cbz(&cbz).unwrap();
        assert_eq!(found.cover, Some((b"one".to_vec(), "image/png".to_string())));
        assert_eq!(found.title.as_deref(), Some("Tintin & Co 3"));
        assert_eq!((found.series.as_deref(), found.series_index), (Some("Tintin & Co"), Some(3.0)));
        assert_eq!(found.author.as_deref(), Some("Hergé"));
        assert_eq!((found.published.as_deref(), found.language.as_deref()), (Some("1946"), Some("fr")));
        assert_eq!(found.subjects, ["Adventure", "Humour"]);

        assert!(inspect_cbz(&zip_of(&[("readme.txt", b"x")])).is_err());
        assert!(inspect_cbz(b"not a zip").is_err());
    }

    /// A real image of the given size, as PNG.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbImage::new(width, height).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn a_comic_becomes_a_fixed_layout_book() {
        let dir = std::env::temp_dir().join(format!("legejo-comic-{}", crate::books::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let (src, dst) = (dir.join("c.cbz"), dir.join("c.kepub.epub"));
        let (wide, tall) = (png(30, 20), png(20, 30));
        std::fs::write(
            &src,
            zip_of(&[
                ("10.png", &wide),
                ("2.png", &tall),
                ("notes.txt", b"x"),
                ("3.jpg", b"not an image at all"),
                ("ComicInfo.xml", b"<ComicInfo><Manga>YesAndRightToLeft</Manga></ComicInfo>"),
            ]),
        )
        .unwrap();
        let meta = ComicMeta { uuid: "abc".into(), title: "Tom & Jerry".into(), author: Some("A <B>".into()), language: None };
        assert_eq!(cbz_to_epub(&src, &dst, &meta).unwrap(), 2, "the broken page is left out");

        let bytes = std::fs::read(&dst).unwrap();
        let mut book = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        // The marker comes first and is not compressed, as readers require.
        let first = book.by_index(0).unwrap();
        assert_eq!((first.name(), first.compression()), ("mimetype", zip::CompressionMethod::Stored));
        drop(first);
        let mut text = |name: &str| {
            let mut s = String::new();
            book.by_name(name).unwrap().read_to_string(&mut s).unwrap();
            s
        };
        let opf = text("OEBPS/content.opf");
        assert!(opf.contains("<dc:title>Tom &amp; Jerry</dc:title>") && opf.contains("<dc:creator>A &lt;B&gt;</dc:creator>"));
        assert!(opf.contains("pre-paginated") && opf.contains("page-progression-direction=\"rtl\""));
        assert!(opf.contains("href=\"images/p0001.png\" media-type=\"image/png\" properties=\"cover-image\""));
        assert_eq!(opf.matches("<itemref").count(), 2);
        // Reading order: 2 before 10; each page is as large as its image.
        assert!(text("OEBPS/p0001.xhtml").contains("content=\"width=20, height=30\""));
        assert!(text("OEBPS/p0002.xhtml").contains("content=\"width=30, height=20\""));
        let mut image = Vec::new();
        book.by_name("OEBPS/images/p0001.png").unwrap().read_to_end(&mut image).unwrap();
        assert_eq!(image, tall, "the image itself is untouched");

        // It reads as an EPUB, with nothing for the file check to mend.
        assert!(epub::doc::EpubDoc::from_reader(Cursor::new(bytes.clone())).is_ok());
        let issues = crate::epubfix::inspect(&bytes).unwrap();
        assert!(issues.iter().all(|i| !i.fixable), "{:?}", issues.iter().map(|i| &i.code).collect::<Vec<_>>());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_pdf_gives_its_title_and_author() {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let image = doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 600, "Height" => 800, "Filter" => "DCTDecode" },
            b"jpeg bytes".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image } },
        });
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        // UTF-16 with its mark, as programs write names outside ASCII.
        let title: Vec<u8> = [0xFE, 0xFF].into_iter().chain("Röda rummet".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        let info = doc.add_object(dictionary! {
            "Title" => Object::String(title, lopdf::StringFormat::Hexadecimal),
            "Author" => Object::string_literal("August  Strindberg"),
            "Keywords" => Object::string_literal("satire; Stockholm"),
        });
        doc.trailer.set("Root", catalog);
        doc.trailer.set("Info", info);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let found = inspect_pdf(&bytes).unwrap();
        assert_eq!(found.title.as_deref(), Some("Röda rummet"));
        assert_eq!(found.author.as_deref(), Some("August Strindberg"));
        assert_eq!(found.subjects, ["satire", "Stockholm"]);
        assert_eq!(found.cover, Some((b"jpeg bytes".to_vec(), "image/jpeg".to_string())));

        // A file that is a PDF in name only is kept, with nothing found.
        let found = inspect_pdf(b"%PDF-1.4 and then nothing a reader understands").unwrap();
        assert!(found.title.is_none() && found.cover.is_none());
        assert!(empty_title("Microsoft Word - thesis.docx") && empty_title("scan0001.pdf") && !empty_title("Dracula"));
    }
}
