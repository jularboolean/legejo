//! Books that are not EPUB: PDF and CBZ files are stored, described and
//! handed out as they are, and left alone by everything that works on EPUB.

use crate::library_tests::{epub, upload};
use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

/// A real image of the given size, as PNG.
fn page(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbImage::new(width, height).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn cbz(title: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    z.start_file("002.png", options).unwrap();
    z.write_all(&page(40, 30)).unwrap();
    z.start_file("001.png", options).unwrap();
    z.write_all(&page(30, 40)).unwrap();
    z.start_file("ComicInfo.xml", options).unwrap();
    write!(z, "<ComicInfo><Title>{title}</Title><Series>Tintin</Series><Number>2</Number><Writer>Herg\u{e9}</Writer></ComicInfo>").unwrap();
    z.finish().unwrap().into_inner()
}

fn pdf(title: &str) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    let info = doc.add_object(dictionary! { "Title" => Object::string_literal(title), "Author" => Object::string_literal("A. Writer") });
    doc.trailer.set("Root", catalog);
    doc.trailer.set("Info", info);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// GET with a session; returns status, content type, disposition and body.
async fn get(app: &axum::Router, uri: &str, cookie: &str) -> (StatusCode, String, String, Vec<u8>) {
    let req = Request::builder().uri(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let header_text = |name: header::HeaderName| res.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let (status, kind, disposition) = (res.status(), header_text(header::CONTENT_TYPE), header_text(header::CONTENT_DISPOSITION));
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, kind, disposition, body)
}

#[tokio::test]
async fn pdf_and_cbz_are_kept_as_they_are() {
    let (app, db, state) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let (pdf_bytes, cbz_bytes) = (pdf("On Ice"), cbz("The Broken Ear"));

    let v = upload(
        &app,
        &a,
        &[
            ("ice.pdf", pdf_bytes.clone()),
            ("ear.cbz", cbz_bytes.clone()),
            ("novel.epub", epub("A Novel", "urn:uuid:1")),
            ("notes.txt", b"plain text".to_vec()),
            ("untitled scan.pdf", b"%PDF-1.4 not much of a document".to_vec()),
        ],
        "",
    )
    .await;
    let added = v["added"].as_array().unwrap();
    let by_format = |format: &str| added.iter().filter(|b| b["format"] == format).collect::<Vec<_>>();
    assert_eq!(added.len(), 4, "{v}");
    assert_eq!(v["errors"].as_array().unwrap().len(), 1, "the text file is refused: {v}");
    assert_eq!((by_format("pdf").len(), by_format("cbz").len(), by_format("epub").len()), (2, 1, 1));

    // What the files say about themselves; the file's name when they say nothing.
    let book = |title: &str| added.iter().find(|b| b["title"] == title).unwrap_or_else(|| panic!("no {title} in {v}")).clone();
    let (ice, ear, scan) = (book("On Ice"), book("The Broken Ear"), book("untitled scan"));
    assert_eq!(ice["author"], "A. Writer");
    assert_eq!((ear["author"].as_str(), ear["series"].as_str(), ear["series_index"].as_f64()), (Some("Hergé"), Some("Tintin"), Some(2.0)));
    assert_eq!((ear["has_cover"].as_bool(), ice["has_cover"].as_bool()), (Some(true), Some(false)));
    assert_eq!(scan["format"], "pdf");
    let (ice_id, ear_id) = (ice["id"].as_i64().unwrap(), ear["id"].as_i64().unwrap());
    let (_, kind, _, cover) = get(&app, &format!("/api/books/{ear_id}/cover"), &a).await;
    assert_eq!((kind.as_str(), cover), ("image/png", page(30, 40)), "the first page in reading order");

    // Stored under their own extension, and handed out unchanged.
    let uuid: String = sqlx::query_scalar("SELECT uuid FROM books WHERE id = ?").bind(ear_id).fetch_one(&db).await.unwrap();
    let file = state.data_dir.join("books").join(format!("{uuid}.cbz"));
    assert!(file.exists());
    let (status, kind, disposition, body) = get(&app, &format!("/api/books/{ear_id}/file"), &a).await;
    assert_eq!((status, kind.as_str()), (StatusCode::OK, "application/vnd.comicbook+zip"));
    assert!(disposition.ends_with("The Broken Ear.cbz\""), "{disposition}");
    assert_eq!(body, cbz_bytes);
    let (_, kind, disposition, body) = get(&app, &format!("/api/books/{ice_id}/file"), &a).await;
    assert_eq!(kind, "application/pdf");
    assert!(disposition.ends_with("On Ice.pdf\""), "{disposition}");
    assert_eq!(body, pdf_bytes);

    // A reading app is told what kind of file each book is, and gets that file.
    {
        use base64::Engine;
        let hash = crate::db::hash_password("opds password").await.unwrap();
        sqlx::query("UPDATE users SET password_hash = ? WHERE username = 'alice'").bind(&hash).execute(&db).await.unwrap();
        let auth = format!("Basic {}", base64::engine::general_purpose::STANDARD.encode("alice:opds password"));
        let fetch = |uri: String| {
            let req = Request::builder().uri(uri).header(header::AUTHORIZATION, auth.clone()).body(Body::empty()).unwrap();
            let app = app.clone();
            async move {
                let res = app.oneshot(req).await.unwrap();
                let kind = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                (res.status(), kind, axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec())
            }
        };
        let (status, _, feed) = fetch("/api/opds/books".to_string()).await;
        let feed = String::from_utf8(feed).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert!(feed.contains(&format!("href=\"/api/opds/books/{ear_id}/file\" type=\"application/vnd.comicbook+zip\"")), "{feed}");
        assert!(feed.contains(&format!("href=\"/api/opds/books/{ice_id}/file\" type=\"application/pdf\"")), "{feed}");
        assert_eq!(feed.matches("type=\"application/epub+zip\"").count(), 1);
        let (status, kind, body) = fetch(format!("/api/opds/books/{ice_id}/file")).await;
        assert_eq!((status, kind.as_str(), body), (StatusCode::OK, "application/pdf", pdf_bytes.clone()));
    }

    // Editing and repairing change the catalog, never the file.
    let (status, v) = send(&app, Method::PUT, &format!("/api/books/{ear_id}"), Some(&a), Some(json!({ "title": "L'Oreille cassée" }))).await;
    assert_eq!((status, v["title"].as_str(), v["format"].as_str()), (StatusCode::OK, Some("L'Oreille cassée"), Some("cbz")));
    let (status, _) = send(&app, Method::POST, &format!("/api/books/{ice_id}/repair"), Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(std::fs::read(&file).unwrap(), cbz_bytes);
    let health: Option<String> = sqlx::query_scalar("SELECT health FROM books WHERE id = ?").bind(ice_id).fetch_one(&db).await.unwrap();
    assert!(health.is_none(), "the file check is one of EPUB files");

    // The same file again is a duplicate, whatever its format.
    let v = upload(&app, &a, &[("again.cbz", cbz_bytes.clone())], "").await;
    assert_eq!((v["added"].as_array().unwrap().len(), v["duplicates"][0]["kind"].as_str()), (0, Some("same_file")));

    // Shared on a shelf, it is copied to another library as the same kind of file.
    let (_, shelf) = send(&app, Method::POST, "/api/shelves", Some(&a), Some(json!({ "name": "Comics" }))).await;
    let shelf = shelf["id"].as_i64().unwrap();
    send(&app, Method::POST, "/api/books/bulk", Some(&a), Some(json!({ "ids": [ear_id], "action": "add_to_shelf", "shelf_id": shelf }))).await;
    send(&app, Method::PUT, &format!("/api/shelves/{shelf}"), Some(&a), Some(json!({ "name": "Comics", "visibility": "instance" }))).await;
    let (status, copy) = send(&app, Method::POST, &format!("/api/public/books/{ear_id}/import"), Some(&b), None).await;
    assert_eq!((status, copy["format"].as_str()), (StatusCode::CREATED, Some("cbz")), "{copy}");
    let (_, kind, _, body) = get(&app, &format!("/api/books/{}/file", copy["id"]), &b).await;
    assert_eq!((kind.as_str(), body), ("application/vnd.comicbook+zip", cbz_bytes.clone()));

    // A Kobo e-reader is offered the EPUB and the comic, the comic as a book
    // made for it; a PDF stays behind.
    let (_, account) = send(&app, Method::POST, "/api/account/kobo-token", Some(&a), None).await;
    let token = account["kobo_token"].as_str().unwrap().to_string();
    let (status, sync) = send(&app, Method::GET, &format!("/api/kobo/{token}/v1/library/sync"), None, None).await;
    assert_eq!(status, StatusCode::OK, "{sync}");
    let offered: Vec<(String, String, String)> = sync
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item.get("NewEntitlement"))
        .map(|e| {
            let (meta, url) = (&e["BookMetadata"], &e["BookMetadata"]["DownloadUrls"][0]);
            (meta["Title"].as_str().unwrap().to_string(), url["Format"].as_str().unwrap().to_string(), url["Url"].as_str().unwrap().to_string())
        })
        .collect();
    let mut titles: Vec<&str> = offered.iter().map(|(title, _, _)| title.as_str()).collect();
    titles.sort();
    assert_eq!(titles, ["A Novel", "L'Oreille cassée"], "{sync}");
    let (_, format, url) = offered.iter().find(|(title, _, _)| title.starts_with("L'Oreille")).unwrap();
    assert_eq!(format, "KEPUB");
    assert!(url.ends_with(".kepub.epub"), "{url}");
    let (status, kind, _, book) = get(&app, &url[url.find("/api/kobo/").unwrap()..], "").await;
    assert_eq!((status, kind.as_str()), (StatusCode::OK, "application/epub+zip"));
    let mut made = zip::ZipArchive::new(std::io::Cursor::new(book)).unwrap();
    assert_eq!(made.file_names().filter(|name| name.starts_with("OEBPS/images/")).count(), 2);
    let mut opf = String::new();
    std::io::Read::read_to_string(&mut made.by_name("OEBPS/content.opf").unwrap(), &mut opf).unwrap();
    assert!(opf.contains("pre-paginated") && opf.contains("<dc:title>L'Oreille cassée</dc:title>"), "{opf}");
    // Made once and kept; the comic itself is as it came.
    assert!(state.data_dir.join("kepub").join(format!("{uuid}.kepub.epub")).exists());
    assert_eq!(std::fs::read(&file).unwrap(), cbz_bytes);

    // Deleting takes the file along.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{ear_id}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!file.exists());
    assert!(!state.data_dir.join("kepub").join(format!("{uuid}.kepub.epub")).exists(), "the book made of it goes too");
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&a), None).await;
    let formats: Vec<&str> = list.as_array().unwrap().iter().map(|b| b["format"].as_str().unwrap()).collect();
    assert_eq!(formats.iter().filter(|f| **f == "pdf").count(), 2);
}

/// The book files are streamed from disk rather than read whole; the
/// receiver is still told the length, and gets every byte.
#[tokio::test]
async fn downloads_are_streamed_with_their_length() {
    let (app, db, _) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let cbz_bytes = cbz("The Broken Ear");
    let v = upload(&app, &a, &[("ear.cbz", cbz_bytes.clone())], "").await;
    let id = v["added"][0]["id"].as_i64().unwrap();
    let uuid: String = sqlx::query_scalar("SELECT uuid FROM books WHERE id = ?").bind(id).fetch_one(&db).await.unwrap();
    let (_, account) = send(&app, Method::POST, "/api/account/kobo-token", Some(&a), None).await;
    let token = account["kobo_token"].as_str().unwrap().to_string();

    let length_and_body = |uri: String| {
        let app = app.clone();
        let a = a.clone();
        async move {
            let req = Request::builder().uri(uri).header(header::COOKIE, a).body(Body::empty()).unwrap();
            let res = app.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let length = res.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).map(|v| v.to_string());
            let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec();
            (length, body)
        }
    };
    let (length, body) = length_and_body(format!("/api/books/{id}/file")).await;
    assert_eq!(length.as_deref(), Some(cbz_bytes.len().to_string().as_str()));
    assert_eq!(body, cbz_bytes);
    // The device gets the comic made into a book, streamed the same way.
    let (length, body) = length_and_body(format!("/api/kobo/{token}/v1/download/{uuid}.kepub.epub")).await;
    assert_eq!(length.as_deref(), Some(body.len().to_string().as_str()));
    assert!(body.starts_with(b"PK"), "a zip, {} bytes", body.len());
}

#[tokio::test]
async fn koreader_and_the_web_share_the_page_of_a_pdf_or_a_comic() {
    use md5::{Digest, Md5};
    let (app, db, _) = test_app().await;
    let (_, cookie) = add_user(&db, "alice").await;
    let v = upload(&app, &cookie, &[("ice.pdf", pdf("On Ice")), ("ear.cbz", cbz("The Broken Ear"))], "").await;
    let id_of = |name: &str| v["added"].as_array().unwrap().iter().find(|b| b["title"] == name).unwrap()["id"].as_i64().unwrap();
    let (pdf_book, cbz_book) = (id_of("On Ice"), id_of("The Broken Ear"));
    let document = |book: i64| {
        let db = db.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT koreader_md5 FROM books WHERE id = ?").bind(book).fetch_one(&db).await.unwrap() }
    };
    let (pdf_doc, cbz_doc) = (document(pdf_book).await, document(cbz_book).await);

    let (_, k) = send(&app, Method::POST, "/api/account/kosync-key", Some(&cookie), None).await;
    let auth_key: String = Md5::digest(k["key"].as_str().unwrap().as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let ko = |method: Method, uri: String, body: Option<serde_json::Value>| {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/kosync{uri}"))
            .header("x-auth-user", "alice")
            .header("x-auth-key", auth_key.clone());
        if body.is_some() {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        let req = req.body(body.map_or(Body::empty(), |b| Body::from(b.to_string()))).unwrap();
        let app = app.clone();
        async move {
            let res = app.oneshot(req).await.unwrap();
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        }
    };
    let web = |book: i64| {
        let (app, cookie) = (app.clone(), cookie.clone());
        async move { send(&app, Method::GET, &format!("/api/books/{book}/progress"), Some(&cookie), None).await.1 }
    };

    // KOReader names a page of a PDF by its number; the web reader opens there.
    ko(Method::PUT, "/syncs/progress".into(), Some(json!({ "document": pdf_doc, "progress": "7", "percentage": 0.7, "device": "Kobo", "device_id": "K1" }))).await;
    assert_eq!(web(pdf_book).await["cfi"], "pdf:7");

    // Read on in the browser, and KOReader is given that page, from the web.
    send(&app, Method::PUT, &format!("/api/books/{pdf_book}/progress"), Some(&cookie), Some(json!({ "cfi": "pdf:9", "percent": 0.8 }))).await;
    let v = ko(Method::GET, format!("/syncs/progress/{pdf_doc}"), None).await;
    assert_eq!((v["progress"].clone(), v["percentage"].clone(), v["device"].clone()), (json!("9"), json!(0.8), json!("Legejo")));
    assert_ne!(v["device_id"], "K1");
    assert!(v["timestamp"].as_i64().unwrap() > 0);

    // A comic read only in the browser: KOReader still finds its page.
    send(&app, Method::PUT, &format!("/api/books/{cbz_book}/progress"), Some(&cookie), Some(json!({ "cfi": "cbz:3", "percent": 0.4 }))).await;
    assert_eq!(ko(Method::GET, format!("/syncs/progress/{cbz_doc}"), None).await["progress"], "3");
    // And KOReader's newer page goes the other way.
    ko(Method::PUT, "/syncs/progress".into(), Some(json!({ "document": cbz_doc, "progress": "5", "percentage": 0.5, "device": "Kobo", "device_id": "K1" }))).await;
    let v = ko(Method::GET, format!("/syncs/progress/{cbz_doc}"), None).await;
    assert_eq!((v["progress"].clone(), v["device_id"].clone()), (json!("5"), json!("K1")));
    assert_eq!(web(cbz_book).await["cfi"], "cbz:5");

    // The comic made into a book for a device is read by text: no page from it.
    let copy = "0123456789abcdef0123456789abcdef";
    sqlx::query("INSERT INTO book_file_ids (md5, book_id) VALUES (?, ?)").bind(copy).bind(cbz_book).execute(&db).await.unwrap();
    ko(Method::PUT, "/syncs/progress".into(), Some(json!({ "document": copy, "progress": "/body/DocFragment[4]/body/img", "percentage": 0.6 }))).await;
    let p = web(cbz_book).await;
    assert_eq!((p["cfi"].clone(), p["percent"].clone()), (serde_json::Value::Null, json!(0.6)));
    assert_eq!(ko(Method::GET, format!("/syncs/progress/{copy}"), None).await["progress"], "/body/DocFragment[4]/body/img");
}
