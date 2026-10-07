//! Books that are not EPUB: PDF and CBZ files are stored, described and
//! handed out as they are, and left alone by everything that works on EPUB.

use crate::library_tests::{epub, upload};
use crate::testutil::{add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

fn cbz(title: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    z.start_file("002.jpg", options).unwrap();
    z.write_all(b"second page").unwrap();
    z.start_file("001.jpg", options).unwrap();
    z.write_all(b"first page").unwrap();
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
    assert_eq!((kind.as_str(), cover.as_slice()), ("image/jpeg", b"first page".as_slice()), "the first page in reading order");

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

    // An e-reader that syncs as a Kobo is offered the EPUB only.
    let offered: Vec<String> = sqlx::query_scalar("SELECT title FROM books WHERE owner_id = (SELECT id FROM users WHERE username = 'alice') AND format = 'epub'")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(offered, ["A Novel"]);

    // Deleting takes the file along.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{ear_id}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!file.exists());
    let (_, list) = send(&app, Method::GET, "/api/books", Some(&a), None).await;
    let formats: Vec<&str> = list.as_array().unwrap().iter().map(|b| b["format"].as_str().unwrap()).collect();
    assert_eq!(formats.iter().filter(|f| **f == "pdf").count(), 2);
}
