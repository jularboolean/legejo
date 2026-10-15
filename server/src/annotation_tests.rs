//! Highlights and notes: the owner's, in the owner's books, and nobody else's.

use crate::annotations::{markdown, Annotation};
use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn highlights_and_notes_belong_to_the_owner_of_the_book() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let (book, _) = add_book(&db, alice, "Röda rummet").await;
    let url = format!("/api/books/{book}/annotations");

    // Nothing yet, and the book page says so.
    let (status, v) = send(&app, Method::GET, &url, Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!([]));
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["annotation_count"], 0);

    // A highlight, and one with a note. The passage is tidied, the note trimmed.
    let (status, h) = send(&app, Method::POST, &url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2,/1:0,/1:9)", "text": "  Det var\n en afton " }))).await;
    assert_eq!(status, StatusCode::CREATED, "{h}");
    assert_eq!(h["text"], "Det var en afton");
    assert_eq!(h["source"], "web");
    assert!(h["note"].is_null());
    let (status, n) = send(&app, Method::POST, &url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/4,/1:0,/1:5)", "text": "Stockholm", "note": "  Början.  " }))).await;
    assert_eq!(status, StatusCode::CREATED, "{n}");
    assert_eq!(n["note"], "Början.");
    let (_, v) = send(&app, Method::GET, &url, Some(&a), None).await;
    assert_eq!(v.as_array().unwrap().len(), 2);
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{book}"), Some(&a), None).await;
    assert_eq!(v["annotation_count"], 2);

    // Bob sees none of it and can change none of it.
    let (status, _) = send(&app, Method::GET, &url, Some(&b), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::POST, &url, Some(&b), Some(json!({ "cfi": "epubcfi(/6/4!/4/2,/1:0,/1:9)", "text": "x" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let hid = h["id"].as_i64().unwrap();
    let (status, _) = send(&app, Method::PUT, &format!("/api/annotations/{hid}"), Some(&b), Some(json!({ "note": "mine now" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/annotations/{hid}"), Some(&b), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The note of a highlight: added, then taken away with an empty one.
    let (status, v) = send(&app, Method::PUT, &format!("/api/annotations/{hid}"), Some(&a), Some(json!({ "note": "En kväll i augusti" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["note"], "En kväll i augusti");
    assert_eq!(v["text"], "Det var en afton", "the passage is not touched");
    let (_, v) = send(&app, Method::PUT, &format!("/api/annotations/{hid}"), Some(&a), Some(json!({}))).await;
    assert_eq!(v["note"], "En kväll i augusti", "absent means unchanged");
    let (_, v) = send(&app, Method::PUT, &format!("/api/annotations/{hid}"), Some(&a), Some(json!({ "note": "" }))).await;
    assert!(v["note"].is_null(), "empty takes the note away");

    // What is refused.
    for body in [
        json!({ "cfi": "", "text": "x" }),
        json!({ "cfi": "epubcfi(/6/4!/4/2)", "text": "   " }),
        json!({ "cfi": "epubcfi(/6/4!/4/2)", "text": "x".repeat(4001) }),
        json!({ "cfi": "epubcfi(/6/4!/4/2)", "text": "x", "note": "n".repeat(10_001) }),
    ] {
        let (status, _) = send(&app, Method::POST, &url, Some(&a), Some(body.clone())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }

    // Deleting one leaves the other; deleting the book takes the rest.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/annotations/{hid}"), Some(&a), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, &url, Some(&a), None).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    sqlx::query("DELETE FROM books WHERE id = ?").bind(book).execute(&db).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM annotations").fetch_one(&db).await.unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn the_export_is_a_markdown_file_of_quotes_and_notes() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (book, _) = add_book(&db, alice, "Röda rummet: en skildring").await;
    sqlx::query("UPDATE books SET author = 'August Strindberg' WHERE id = ?").bind(book).execute(&db).await.unwrap();
    let url = format!("/api/books/{book}/annotations");
    send(&app, Method::POST, &url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/2,/1:0,/1:9)", "text": "Det var en afton i början af maj." }))).await;
    send(&app, Method::POST, &url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/4!/4/4,/1:0,/1:5)", "text": "Stockholm", "note": "Staden som huvudperson." }))).await;

    let req = Request::builder().uri(format!("{url}/export")).header(header::COOKIE, &a).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "text/markdown; charset=utf-8");
    let disposition = res.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
    assert!(disposition.contains("R%C3%B6da%20rummet"), "{disposition}");
    let body = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert_eq!(
        body,
        "# Röda rummet: en skildring\n\nAugust Strindberg\n\n> Det var en afton i början af maj.\n\n> Stockholm\n\nStaden som huvudperson.\n"
    );
}

#[test]
fn a_passage_of_several_lines_is_quoted_line_by_line() {
    let a = Annotation {
        id: 1,
        book_id: 1,
        source: "web".into(),
        cfi: None,
        text: "En rad.\nEn till.".into(),
        note: Some("  ok  ".into()),
        color: None,
        location: None,
        place: None,
        created_at: String::new(),
        updated_at: String::new(),
    };
    assert_eq!(markdown("Bok", None, &[a]), "# Bok\n\n> En rad.\n> En till.\n\nok\n");
}

#[tokio::test]
async fn all_of_a_users_notes_come_together_with_their_books() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (bob, b) = add_user(&db, "bob").await;
    let (rr, _) = add_book(&db, alice, "Röda rummet").await;
    let (hb, _) = add_book(&db, alice, "Hemsöborna").await;
    let (bobs, _) = add_book(&db, bob, "Bobs bok").await;
    for (book, cookie, text, note) in [
        (rr, &a, "Det var en afton", None),
        (hb, &a, "Han kom som ett yrväder", Some("Början.")),
        (bobs, &b, "Bobs mening", None),
    ] {
        let (status, _) = send(&app, Method::POST, &format!("/api/books/{book}/annotations"), Some(cookie), Some(json!({ "cfi": "epubcfi(/6/4!/4/2,/1:0,/1:9)", "text": text, "note": note }))).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    // Alice gets hers, each with its book; not Bob's.
    let (status, v) = send(&app, Method::GET, "/api/annotations", Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 2, "{v}");
    let hems = list.iter().find(|n| n["book_id"] == hb).unwrap();
    assert_eq!(hems["book_title"], "Hemsöborna");
    assert_eq!(hems["note"], "Början.");
    assert_eq!(hems["book_has_cover"], false);
    assert!(hems["book_changed_at"].is_string());
    assert!(list.iter().all(|n| n["book_id"] != bobs));
    let (status, _) = send(&app, Method::GET, "/api/annotations", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // One Markdown file with every book that has any, by title.
    let req = Request::builder().uri("/api/annotations/export").header(header::COOKIE, &a).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("notes.md"));
    let body = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    let (h, r) = (body.find("# Hemsöborna").unwrap(), body.find("# Röda rummet").unwrap());
    assert!(h < r, "{body}");
    assert!(body.contains("> Han kom som ett yrväder") && body.contains("Början."));
    assert!(!body.contains("Bobs"));
}

#[tokio::test]
async fn a_highlight_knows_its_chapter_share_and_page_without_seeming_changed() {
    let (app, db, _) = test_app().await;
    let (alice, a) = add_user(&db, "alice").await;
    let (book, _) = add_book(&db, alice, "Röda rummet").await;
    let url = format!("/api/books/{book}/annotations");

    // Made with its place; what is not a place is left out.
    let (status, h) = send(&app, Method::POST, &url, Some(&a), Some(json!({
        "cfi": "epubcfi(/6/4!/4/2,/1:0,/1:9)", "text": "Det var en afton",
        "place": { "chapter": " Första kapitlet ", "percent": 0.12345, "page": 7, "secret": "x" }
    }))).await;
    assert_eq!(status, StatusCode::CREATED, "{h}");
    assert_eq!(h["place"], json!({ "chapter": "Första kapitlet", "percent": 0.123, "page": "7" }));

    // One made before places were kept is given its place later, and does
    // not count as changed: a device goes by updated_at.
    let (_, old) = send(&app, Method::POST, &url, Some(&a), Some(json!({ "cfi": "epubcfi(/6/8!/4/2,/1:0,/1:5)", "text": "Stockholm" }))).await;
    assert!(old["place"].is_null());
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let (status, placed) = send(&app, Method::PUT, &format!("/api/annotations/{}", old["id"]), Some(&a), Some(json!({ "place": { "chapter": "Andra", "percent": 0.5 } }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(placed["place"]["chapter"], "Andra");
    assert_eq!(placed["updated_at"], old["updated_at"]);
    // Nonsense is no place, and changes nothing.
    let (_, same) = send(&app, Method::PUT, &format!("/api/annotations/{}", old["id"]), Some(&a), Some(json!({ "place": { "percent": 7 } }))).await;
    assert_eq!(same["place"]["chapter"], "Andra");

    // The list of all of them carries it.
    let (_, all) = send(&app, Method::GET, "/api/annotations", Some(&a), None).await;
    assert!(all.as_array().unwrap().iter().any(|n| n["place"]["page"] == "7"));
}
