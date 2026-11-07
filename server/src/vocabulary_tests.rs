//! A book's vocabulary: the language from the EPUB brought to the list's
//! code, the words counted once per file in the background, the study list
//! of an Esperanto book with ESPDIC's English, and the reader's known words.

use crate::library_tests::upload;
use crate::testutil::{add_user, send, test_app};
use crate::vocabulary;
use axum::http::{Method, StatusCode};
use serde_json::json;

/// An EPUB in a language, its one chapter this text.
fn book(title: &str, language: &str, text: &str) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut file = |name: &str, content: String| {
        z.start_file(name, stored).unwrap();
        z.write_all(content.as_bytes()).unwrap();
    };
    file("mimetype", "application/epub+zip".into());
    file(
        "META-INF/container.xml",
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="c.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into(),
    );
    file(
        "c.opf",
        format!(
            r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="i"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{title}</dc:title><dc:language>{language}</dc:language><dc:identifier id="i">urn:test:{title}</dc:identifier></metadata><manifest><item id="c1" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/></spine></package>"#
        ),
    );
    file("one.xhtml", format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>x</title></head><body><p>{text}</p></body></html>"#));
    z.finish().unwrap().into_inner()
}

fn esperanto() -> String {
    let mut text = String::new();
    for i in 0..200 {
        text.push_str("La hundo diris al Petro ke la kato bone dormas. ");
        if i % 5 == 0 {
            text.push_str("Petro respondis ke la domo estas granda. ");
        }
    }
    text
}

#[tokio::test]
async fn an_esperanto_book_gets_its_study_list_and_the_readers_known_words_count() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let (_, bob) = add_user(&db, "bob").await;
    // The EPUB says "epo"; the catalog keeps "eo".
    let up = upload(&app, &alice, &[("hundo.epub", book("Hundo", "epo", &esperanto()))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    let (_, b) = send(&app, Method::GET, &format!("/api/books/{id}"), Some(&alice), None).await;
    assert_eq!(b["language"], "eo", "{b}");
    // Bob has the same file: it is counted once.
    upload(&app, &bob, &[("hundo.epub", book("Hundo", "epo", &esperanto()))], "").await;

    // Asked before the worker came by: counted there and then, once for both copies.
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 0);

    let (status, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["language"], "eo");
    assert!(v["words"].as_i64().unwrap() > 2000);
    assert!(v["difficulty"].is_null(), "one book in the language: no rank");
    assert_eq!(v["short"], true);
    assert_eq!(v["names"], json!(["Petro"]));
    let study = v["study"].as_array().unwrap();
    assert_eq!(study[0]["word"], "la");
    let hundo = study.iter().find(|w| w["word"] == "hundo").unwrap();
    assert_eq!(hundo["english"], "dog, hound");
    assert_eq!(hundo["class"], "noun");
    assert_eq!(study.iter().find(|w| w["word"] == "diri").unwrap()["class"], "verb");
    assert_eq!(study[0]["class"], "other", "la");
    assert_eq!(hundo["known"], false);
    let diri = study.iter().find(|w| w["word"] == "diri").unwrap();
    assert!(diri["english"].as_str().unwrap().contains("say"));
    let before = v["known_share"].as_f64().unwrap();
    assert_eq!(before, 0.0, "nothing ticked yet: 0 %, names aside");

    // Alice knows "la" and "hundo": they cover more of the book now; Bob's list is his own.
    for stem in ["la", "hund"] {
        let (status, _) = send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "eo", "stem": stem, "known": true }))).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert!(v["known_share"].as_f64().unwrap() > before + 0.1, "{}", v["known_share"]);
    let (_, known) = send(&app, Method::GET, "/api/vocabulary/known?language=eo", Some(&alice), None).await;
    assert_eq!(known["stems"].as_array().unwrap().len(), 2);
    let (_, known) = send(&app, Method::GET, "/api/vocabulary/known?language=eo", Some(&bob), None).await;
    assert_eq!(known["stems"], json!([]));
    send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "eo", "stem": "la", "known": false }))).await;
    let (_, known) = send(&app, Method::GET, "/api/vocabulary/known?language=eo", Some(&alice), None).await;
    assert_eq!(known["stems"], json!(["hund"]));

    // Someone else's book is not shown.
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_book_in_another_language_has_statistics_but_no_study_list() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let text = "Det var en gång en katt som hette Måns. Katten sov i solen och drömde om fisk. ".repeat(100);
    let up = upload(&app, &alice, &[("katt.epub", book("Katt", "sv-SE", &text))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    vocabulary::count_pending(&state, 10).await.unwrap();
    let (status, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["language"], "sv");
    assert!(v["study"].is_null());
    assert!(v["known_share"].is_null());
    assert_eq!(v["names"], json!(["Måns"]));
    // One book in Swedish: its commonest words, without "och" or "en";
    // nothing to set characteristic words against yet.
    let common: Vec<&str> = v["common"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    assert!(common.contains(&"katt") || common.contains(&"katten"), "{common:?}");
    assert!(!common.contains(&"och") && !common.contains(&"en") && !common.contains(&"som"), "{common:?}");
    assert_eq!(v["characteristic"], json!([]));
    assert!(v["coverage"]["90"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn languages_already_in_the_catalog_are_brought_to_the_list() {
    let (_, db, state) = test_app().await;
    let (alice, _) = add_user(&db, "alice").await;
    for (title, language) in [("a", Some("swe")), ("b", Some("en-GB")), ("c", Some("UND")), ("d", Some("tlh")), ("e", None)] {
        sqlx::query("INSERT INTO books (uuid, owner_id, title, file_size, language) VALUES ($1, $2, $3, 1, $4)")
            .bind(format!("u-{title}"))
            .bind(alice)
            .bind(title)
            .bind(language)
            .execute(&db)
            .await
            .unwrap();
    }
    assert_eq!(crate::language::normalize_catalog(&state).await.unwrap(), 3);
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT title, language FROM books ORDER BY title").fetch_all(&db).await.unwrap();
    assert_eq!(
        rows,
        vec![
            ("a".into(), Some("sv".into())),
            ("b".into(), Some("en".into())),
            ("c".into(), None),
            ("d".into(), Some("tlh".into())),
            ("e".into(), None)
        ]
    );
}

#[tokio::test]
async fn a_search_for_one_form_finds_the_others() {
    let (_, db, state) = test_app().await;
    let mut settings = (*state.settings).clone();
    settings.fulltext = Some(true);
    let mut state = state.clone();
    state.settings = std::sync::Arc::new(settings);
    let app = crate::router(state.clone());
    let (_, alice) = add_user(&db, "alice").await;
    let text = "Hunden skällde på grannen hela natten. ".repeat(20);
    upload(&app, &alice, &[("hund.epub", book("Hund", "sv", &text))], "").await;
    crate::fulltext::index_pending(&state, 10).await.unwrap();
    // "hundarna" is nowhere in the book, "hunden" is.
    let (status, v) = send(&app, Method::GET, "/api/search/text?q=hundarna", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["books"].as_array().unwrap().len(), 1, "{v}");
    assert!(v["books"][0]["passages"][0]["text"].as_str().unwrap().contains("\u{E000}Hunden\u{E001}"), "{v}");
}

#[tokio::test]
async fn a_spanish_book_gets_a_study_list_by_lemma_from_the_built_in_pack() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let text = "Ayer Juan hizo la cena. Luego quiso ir a la casa de su madre, y fue a pie. \
                Hoy hace frío y Juan quiere dormir. La casa es grande. "
        .repeat(60);
    let up = upload(&app, &alice, &[("cena.epub", book("Cena", "es-ES", &text))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    let (status, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["language"], "es");
    let study = v["study"].as_array().unwrap_or_else(|| panic!("no study list: {v}"));
    let row = |word: &str| study.iter().find(|w| w["word"] == word).unwrap_or_else(|| panic!("{word} not in {study:?}"));
    // "hizo" and "hace" are both "hacer"; "quiso" and "quiere" "querer".
    let hacer = row("hacer");
    assert!(hacer["forms"].as_array().unwrap().iter().any(|f| f == "hizo"), "{hacer}");
    assert!(hacer["english"].as_str().unwrap().contains("to do"), "{hacer}");
    assert!(row("querer")["forms"].as_array().unwrap().iter().any(|f| f == "quiso"));
    // A little word is itself, with its own meaning.
    assert!(row("a")["english"].as_str().unwrap().starts_with("to"), "{}", row("a"));
    assert_eq!(row("Juan")["name"], true);
    assert_eq!(hacer["class"], "verb");
    assert_eq!(row("casa")["class"], "noun");
    assert_eq!(row("grande")["class"], "adj");
    assert_eq!(row("a")["class"], "other");
    assert!(v["study_source"].as_str().unwrap().contains("Wiktionary"));

    // Selected in the reader: "hizo" is "hacer", a verb, not known yet; then known.
    let (status, w) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary/word?w=Hizo,"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{w}");
    assert_eq!(w["word"], "hacer");
    assert_eq!(w["class"], "verb");
    assert_eq!(w["known"], false);
    send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "es", "stem": w["stem"], "known": true }))).await;
    let (_, w) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary/word?w=hace"), Some(&alice), None).await;
    assert_eq!(w["word"], "hacer", "{w}");
    assert_eq!(w["known"], true);
    // A name is not a word to learn.
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary/word?w=Juan"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_words_page_gathers_the_readers_books_and_words_in_a_language() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let a = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let b_text = "La kato dormas sur la tablo. La hundo rigardas la katon. ".repeat(150);
    let b = upload(&app, &alice, &[("kato.epub", book("Kato", "eo", &b_text))], "").await;
    let (a, b) = (a["added"][0]["id"].as_i64().unwrap(), b["added"][0]["id"].as_i64().unwrap());
    for id in [a, b] {
        send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    }
    send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "eo", "stem": "la", "known": true }))).await;

    let (status, langs) = send(&app, Method::GET, "/api/vocabulary/languages", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{langs}");
    assert_eq!(langs, json!([{ "language": "eo", "books": 2, "known": 1 }]));

    let (status, v) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let books = v["books"].as_array().unwrap();
    assert_eq!(books.len(), 2);
    assert!(books[0]["share"].as_f64().unwrap() >= books[1]["share"].as_f64().unwrap());
    assert_eq!(v["known"], 1);
    assert_eq!(v["common"]["known"], 1);
    assert!(v["common"]["of"].as_u64().unwrap() > 1, "{}", v["common"]);
    // The known words come a page at a time, the last ticked first.
    send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "eo", "stem": "hund", "known": true }))).await;
    let (status, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&limit=1", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{k}");
    assert_eq!(k["total"], 2);
    assert_eq!(k["words"].as_array().unwrap().len(), 1);
    assert_eq!(k["words"][0]["word"], "hundo", "{k}");
    assert_eq!(k["words"][0]["class"], "noun");
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&offset=1&limit=1", Some(&alice), None).await;
    assert_eq!(k["words"][0]["word"], "la");
    assert_eq!(k["words"][0]["english"], "the");
    assert_eq!(k["words"][0]["books"].as_array().unwrap().len(), 2);
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&sort=alpha", Some(&alice), None).await;
    let all: Vec<&str> = k["words"].as_array().unwrap().iter().map(|w| w["word"].as_str().unwrap()).collect();
    assert_eq!(all, ["hundo", "la"]);
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&q=the", Some(&alice), None).await;
    assert_eq!(k["total"], 1, "{k}");
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&class=noun", Some(&alice), None).await;
    assert_eq!(k["total"], 1, "{k}");
    assert_eq!(k["words"][0]["word"], "hundo");
    let (_, m) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo&q=hund", Some(&alice), None).await;
    assert_eq!(m["matches"][0]["known"], true, "{m}");
    // Next: in both books first ("hundo", "kato"), not "la", not a name.
    let next: Vec<&str> = v["next"].as_array().unwrap().iter().map(|w| w["word"].as_str().unwrap()).collect();
    assert!(!next.contains(&"la") && !next.contains(&"Petro"), "{next:?}");
    assert_eq!(v["next"][0]["books"].as_array().unwrap().len(), 2, "{}", v["next"][0]);
    assert_eq!(v["next"][0]["known"], false);
}

