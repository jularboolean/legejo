//! A book's vocabulary: the language from the EPUB brought to the list's
//! code, the words counted once per file in the background, the study list
//! of an Esperanto book with ESPDIC's English or of a book in a language
//! with a pack, and the reader's known words.

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
    assert_eq!(k["words"][0]["books"], 2);
    assert_eq!(k["words"][0]["book_ids"].as_array().unwrap().len(), 2);
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&sort=alpha", Some(&alice), None).await;
    let all: Vec<&str> = k["words"].as_array().unwrap().iter().map(|w| w["word"].as_str().unwrap()).collect();
    assert_eq!(all, ["hundo", "la"]);
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&q=the", Some(&alice), None).await;
    assert_eq!(k["total"], 1, "{k}");
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo&class=noun", Some(&alice), None).await;
    assert_eq!(k["total"], 1, "{k}");
    assert_eq!(k["words"][0]["word"], "hundo");
    // Example sentences, from both books, each sentence once.
    let (status, e) = send(&app, Method::GET, "/api/vocabulary/examples?language=eo&stem=hund", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{e}");
    let examples = e["examples"].as_array().unwrap();
    assert_eq!(examples.len(), 2, "{e}");
    let books: std::collections::HashSet<i64> = examples.iter().map(|x| x["book"].as_i64().unwrap()).collect();
    assert_eq!(books.len(), 2, "{e}");
    assert!(examples[0]["pieces"].as_array().unwrap().iter().any(|p| p["hit"] == true && p["text"] == "hundo"), "{e}");
    let (_, e) = send(&app, Method::GET, "/api/vocabulary/examples?language=eo&stem=nenio", Some(&alice), None).await;
    assert_eq!(e["examples"], json!([]));
    let (status, m) = send(&app, Method::GET, "/api/vocabulary/search?language=eo&q=hund", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{m}");
    assert_eq!(m["matches"][0]["known"], true, "{m}");
    // An export (no limit) comes without the books of each word.
    let (_, all) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo", Some(&alice), None).await;
    assert_eq!(all["words"][0]["book_ids"], json!([]), "{all}");
    assert!(all["words"][0]["books"].as_i64().unwrap() >= 1);

    // The book being read comes first, then the one known most of.
    send(&app, Method::PUT, &format!("/api/books/{b}/progress"), Some(&alice), Some(json!({ "cfi": "epubcfi(/6/2)", "percent": 0.3 }))).await;
    let (status, list) = send(&app, Method::GET, "/api/vocabulary/books?language=eo&limit=1", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["total"], 2);
    assert_eq!(list["books"][0]["id"], b, "{list}");
    assert_eq!(list["books"][0]["reading"], true);
    let (_, list) = send(&app, Method::GET, "/api/vocabulary/books?language=eo&offset=1", Some(&alice), None).await;
    assert_eq!(list["books"][0]["id"], a, "{list}");

    // Many at once, as "tick all above".
    let (status, _) = send(&app, Method::PUT, "/api/vocabulary/known/many", Some(&alice),
        Some(json!({ "language": "eo", "stems": ["kat", "dorm", "bon"], "known": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(o["known"], 5, "{o}");
    send(&app, Method::PUT, "/api/vocabulary/known/many", Some(&alice), Some(json!({ "language": "eo", "stems": ["kat", "dorm", "bon"], "known": false }))).await;
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(o["known"], 2);

    // The level test: words from the band of the reader's books; knowing all
    // asked ticks the whole band, and a word answered "no" stays unticked.
    let (status, level) = send(&app, Method::GET, "/api/vocabulary/level?language=eo", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{level}");
    let band = &level["bands"][0];
    assert_eq!(band["from"], 0);
    let asked: Vec<&str> = band["words"].as_array().unwrap().iter().map(|w| w["stem"].as_str().unwrap()).collect();
    assert!(!asked.is_empty() && asked.len() <= 8, "{level}");
    let la = band["words"].as_array().unwrap().iter().find(|w| w["word"] == "la");
    assert!(la.is_none_or(|w| w["english"] == "the"), "{level}");
    let words = level["words"].as_u64().unwrap();
    let mut answers: Vec<serde_json::Value> = asked.iter().map(|s| json!({ "stem": s, "known": true })).collect();
    // The word answered "no" is one not ticked before ("la" and "hund" are, and stay so).
    let no = asked.iter().position(|s| !["la", "hund"].contains(s)).unwrap_or_else(|| panic!("{asked:?}"));
    answers[no] = json!({ "stem": asked[no], "known": false });
    let (status, done) = send(&app, Method::POST, "/api/vocabulary/level", Some(&alice), Some(json!({ "language": "eo", "answers": answers }))).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert!(done["estimate"].as_u64().unwrap() >= 1, "{done}");
    let (_, k) = send(&app, Method::GET, "/api/vocabulary/known/words?language=eo", Some(&alice), None).await;
    let known: Vec<&str> = k["words"].as_array().unwrap().iter().map(|w| w["stem"].as_str().unwrap()).collect();
    assert!(!known.contains(&asked[no]), "{known:?}");
    assert!(asked.iter().enumerate().filter(|(i, _)| *i != no).all(|(_, s)| known.contains(s)), "{known:?}");
    assert!(known.len() as u64 <= words + 2);
    // Next: in both books first ("hundo", "kato"), not "la", not a name.
    let next: Vec<&str> = v["next"].as_array().unwrap().iter().map(|w| w["word"].as_str().unwrap()).collect();
    assert!(!next.contains(&"la") && !next.contains(&"Petro"), "{next:?}");
    assert_eq!(v["next"][0]["books"], 2, "{}", v["next"][0]);
    assert_eq!(v["next"][0]["book_ids"].as_array().unwrap().len(), 2, "{}", v["next"][0]);
    assert_eq!(v["next"][0]["known"], false);
}

/// The reader's summary in a language, as word_summaries has it:
/// (books, indexed_at, known_dirty, known).
async fn summary_row(db: &sqlx::AnyPool, user: i64, language: &str) -> Option<(i64, String, i64, i64)> {
    sqlx::query_as("SELECT books, indexed_at, known_dirty, known FROM word_summaries WHERE user_id = $1 AND language = $2")
        .bind(user)
        .bind(language)
        .fetch_optional(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_words_page_summary_is_rebuilt_when_the_library_changes() {
    let (app, db, state) = test_app().await;
    let (alice_id, alice) = add_user(&db, "alice").await;
    let a = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let a = a["added"][0]["id"].as_i64().unwrap();
    send(&app, Method::GET, &format!("/api/books/{a}/vocabulary"), Some(&alice), None).await;
    assert!(summary_row(&db, alice_id, "eo").await.is_none(), "nothing read yet");
    let (status, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{o}");
    assert_eq!(o["books_total"], 1);
    let first = summary_row(&db, alice_id, "eo").await.expect("built on the first read");
    assert_eq!(first.0, 1);
    let (sums,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM word_sums WHERE user_id = $1").bind(alice_id).fetch_one(&db).await.unwrap();
    assert!(sums > 5, "{sums}");

    // Another book, counted: the next read sees it, no worker in between.
    let b_text = "La kato dormas sur la tablo. La hundo rigardas la katon. ".repeat(150);
    let b = upload(&app, &alice, &[("kato.epub", book("Kato", "eo", &b_text))], "").await;
    let b = b["added"][0]["id"].as_i64().unwrap();
    send(&app, Method::GET, &format!("/api/books/{b}/vocabulary"), Some(&alice), None).await;
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(o["books_total"], 2, "{o}");
    assert_eq!(o["next"][0]["books"], 2, "a word of both books first: {}", o["next"][0]);
    let second = summary_row(&db, alice_id, "eo").await.unwrap();
    assert_eq!(second.0, 2);
    assert_ne!(second.1, first.1);

    // A book counted again (as a new version would): the summary follows.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    sqlx::query("UPDATE vocabulary SET version = 0 WHERE language = 'eo' AND file_sha256 = (SELECT file_sha256 FROM books WHERE id = $1)")
        .bind(a)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 1);
    assert_eq!(summary_row(&db, alice_id, "eo").await.unwrap(), second, "not before it is read");
    send(&app, Method::GET, "/api/vocabulary/books?language=eo", Some(&alice), None).await;
    let third = summary_row(&db, alice_id, "eo").await.unwrap();
    assert_eq!(third.0, 2);
    assert_ne!(third.1, second.1, "the latest indexing moved");
    let (latest,): (String,) = sqlx::query_as("SELECT MAX(computed_at) FROM vocabulary_indexed WHERE language = 'eo'").fetch_one(&db).await.unwrap();
    assert_eq!(third.1, latest);

    // A book gone: the next read drops it.
    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{b}"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(o["books_total"], 1, "{o}");
    assert_eq!(o["next"][0]["books"], 1, "{}", o["next"][0]);
    assert_eq!(summary_row(&db, alice_id, "eo").await.unwrap().0, 1);
}

/// A book's share as summed over its words there and then:
/// (learnable, known_n), the way the summary's rows are built.
async fn live_share(db: &sqlx::AnyPool, user: i64, book: i64) -> (i64, i64) {
    sqlx::query_as(
        "SELECT v.words - CAST(COALESCE(SUM(CASE WHEN w.name = 1 THEN w.n ELSE 0 END), 0) AS BIGINT),
                CAST(COALESCE(SUM(CASE WHEN w.name = 0 AND k.stem IS NOT NULL THEN w.n ELSE 0 END), 0) AS BIGINT)
         FROM books b
         JOIN vocabulary v ON v.file_sha256 = b.file_sha256 AND v.language = b.language
         LEFT JOIN vocabulary_words w ON w.file_sha256 = b.file_sha256 AND w.language = b.language
         LEFT JOIN known_words k ON k.user_id = $1 AND k.language = w.language AND k.stem = w.stem
         WHERE b.id = $2 GROUP BY v.words",
    )
    .bind(user)
    .bind(book)
    .fetch_one(db)
    .await
    .unwrap()
}

async fn stored_share(db: &sqlx::AnyPool, user: i64, book: i64) -> (i64, i64) {
    sqlx::query_as("SELECT learnable, known_n FROM book_known WHERE user_id = $1 AND book_id = $2")
        .bind(user)
        .bind(book)
        .fetch_one(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_tick_updates_the_book_share_exactly() {
    let (app, db, _state) = test_app().await;
    let (alice_id, alice) = add_user(&db, "alice").await;
    let a = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let b_text = "La kato dormas sur la tablo. La hundo rigardas la katon. ".repeat(150);
    let b = upload(&app, &alice, &[("kato.epub", book("Kato", "eo", &b_text))], "").await;
    let (a, b) = (a["added"][0]["id"].as_i64().unwrap(), b["added"][0]["id"].as_i64().unwrap());
    for id in [a, b] {
        send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    }
    let (status, _) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
    let before = (stored_share(&db, alice_id, a).await, stored_share(&db, alice_id, b).await);
    assert_eq!(before.0, live_share(&db, alice_id, a).await);
    assert_eq!(before.1, live_share(&db, alice_id, b).await);
    assert_eq!(before.0 .1, 0, "nothing known yet");

    // "hundo" is in both books, "tablo" in one: each share moves by the
    // book's own uses, and only those books.
    let tick = |stem: &'static str, known: bool| {
        let (app, alice) = (app.clone(), alice.clone());
        async move {
            let (status, _) = send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "eo", "stem": stem, "known": known }))).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
    };
    tick("hund", true).await;
    let after_hund = (stored_share(&db, alice_id, a).await, stored_share(&db, alice_id, b).await);
    assert_eq!(after_hund.0, live_share(&db, alice_id, a).await);
    assert_eq!(after_hund.1, live_share(&db, alice_id, b).await);
    assert!(after_hund.0 .1 > 0 && after_hund.1 .1 > 0, "{after_hund:?}");
    assert_eq!(after_hund.0 .0, before.0 .0, "what there is to learn does not move");
    tick("tabl", true).await;
    assert_eq!(stored_share(&db, alice_id, a).await, after_hund.0, "not in this book");
    assert_eq!(stored_share(&db, alice_id, b).await, live_share(&db, alice_id, b).await);
    assert!(stored_share(&db, alice_id, b).await.1 > after_hund.1 .1);
    // Ticking it again changes nothing.
    tick("tabl", true).await;
    assert_eq!(stored_share(&db, alice_id, b).await, live_share(&db, alice_id, b).await);
    tick("tabl", false).await;
    tick("hund", false).await;
    assert_eq!((stored_share(&db, alice_id, a).await, stored_share(&db, alice_id, b).await), before, "unticked: as before");
    // The page shows the stored shares.
    tick("hund", true).await;
    let (_, list) = send(&app, Method::GET, "/api/vocabulary/books?language=eo", Some(&alice), None).await;
    let (learnable, known) = stored_share(&db, alice_id, a).await;
    let shown = list["books"].as_array().unwrap().iter().find(|x| x["id"] == a).unwrap();
    assert!((shown["share"].as_f64().unwrap() - known as f64 / learnable as f64).abs() < 1e-9, "{list}");

    // Many at once: the shares are marked to be built again, and the next
    // read builds them.
    let (status, _) = send(&app, Method::PUT, "/api/vocabulary/known/many", Some(&alice), Some(json!({ "language": "eo", "stems": ["la", "dorm"], "known": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(summary_row(&db, alice_id, "eo").await.unwrap().2, 1, "known_dirty");
    assert_ne!(stored_share(&db, alice_id, a).await, live_share(&db, alice_id, a).await, "not yet");
    send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    let row = summary_row(&db, alice_id, "eo").await.unwrap();
    assert_eq!(row.2, 0);
    assert_eq!(row.3, 3, "la, dorm, hund");
    assert_eq!(stored_share(&db, alice_id, a).await, live_share(&db, alice_id, a).await);
    assert_eq!(stored_share(&db, alice_id, b).await, live_share(&db, alice_id, b).await);
    // A change made behind the summary's back (another process) is noticed
    // by the number of known words.
    sqlx::query("INSERT INTO known_words (user_id, language, stem) VALUES ($1, 'eo', 'kat')").bind(alice_id).execute(&db).await.unwrap();
    send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(stored_share(&db, alice_id, b).await, live_share(&db, alice_id, b).await);
    assert_eq!(summary_row(&db, alice_id, "eo").await.unwrap().3, 4);
}

async fn df(db: &sqlx::AnyPool, language: &str, stem: &str) -> Option<i64> {
    sqlx::query_as::<_, (i64,)>("SELECT docs FROM vocabulary_df WHERE language = $1 AND stem = $2")
        .bind(language)
        .bind(stem)
        .fetch_optional(db)
        .await
        .unwrap()
        .map(|(d,)| d)
}

#[tokio::test]
async fn orphaned_rows_go_in_batches() {
    let (app, db, state) = test_app().await;
    let (alice_id, alice) = add_user(&db, "alice").await;
    let a = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let b_text = "La kato dormas sur la tablo. La hundo rigardas la katon. ".repeat(150);
    let b = upload(&app, &alice, &[("kato.epub", book("Kato", "eo", &b_text))], "").await;
    let (a, b) = (a["added"][0]["id"].as_i64().unwrap(), b["added"][0]["id"].as_i64().unwrap());
    for id in [a, b] {
        send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    }
    send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(df(&db, "eo", "hund").await, Some(2), "a candidate of both books");
    assert_eq!(df(&db, "eo", "tabl").await, Some(1));
    let (sha,): (String,) = sqlx::query_as("SELECT file_sha256 FROM books WHERE id = $1").bind(b).fetch_one(&db).await.unwrap();
    let rows = |table: &'static str| {
        let (db, sha) = (db.clone(), sha.clone());
        async move {
            sqlx::query_as::<_, (i64,)>(&format!("SELECT COUNT(*) FROM {table} WHERE file_sha256 = $1 AND language = 'eo'"))
                .bind(&sha)
                .fetch_one(&db)
                .await
                .unwrap()
                .0
        }
    };
    assert!(rows("vocabulary_words").await > 0);
    assert_eq!(vocabulary::drop_orphans(&state).await.unwrap(), 0, "nothing orphaned yet");

    let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{b}"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(vocabulary::drop_orphans(&state).await.unwrap(), 1);
    for table in ["vocabulary", "vocabulary_words", "vocabulary_indexed"] {
        assert_eq!(rows(table).await, 0, "{table}");
    }
    assert_eq!(df(&db, "eo", "hund").await, Some(1), "one book fewer");
    assert_eq!(df(&db, "eo", "tabl").await, None, "no book left: forgotten");
    // The other book's rows stay, and its share; the gone book's share goes.
    let (kept,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vocabulary_words WHERE language = 'eo'").fetch_one(&db).await.unwrap();
    assert!(kept > 0);
    let (shares,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM book_known WHERE user_id = $1").bind(alice_id).fetch_one(&db).await.unwrap();
    assert_eq!(shares, 1);
    assert_eq!(vocabulary::drop_orphans(&state).await.unwrap(), 0);
    // A reader who is gone takes their summary along.
    sqlx::query("DELETE FROM users WHERE id = $1").bind(alice_id).execute(&db).await.unwrap();
    vocabulary::drop_orphans(&state).await.unwrap();
    for table in ["word_sums", "word_summaries", "book_known"] {
        let (n,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&db).await.unwrap();
        assert_eq!(n, 0, "{table}");
    }
}

#[tokio::test]
async fn characteristic_words_come_from_the_df_table() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    // Five books sharing their content words; the fifth alone has "zorbalo"
    // (often enough to be a candidate, not among its commonest).
    let shared = "La granda hundo kuras rapide al la bela domo kaj la kato dormas sur la tablo dum la birdo kantas en la arbo kaj la fiŝo naĝas. ";
    let mut ids = Vec::new();
    for i in 1..=5 {
        let mut text = shared.repeat(50);
        text.push_str(&format!("Libro numero {i} finiĝas tie. "));
        if i == 5 {
            text.push_str(&"La zorbalo estas verda. ".repeat(6));
        }
        let up = upload(&app, &alice, &[(&format!("l{i}.epub"), book(&format!("Libro {i}"), "eo", &text))], "").await;
        let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
        send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
        ids.push(id);
    }
    assert_eq!(df(&db, "eo", "hund").await, Some(5));
    assert_eq!(df(&db, "eo", "zorbal").await, Some(1));
    let (status, v) = send(&app, Method::GET, &format!("/api/books/{}/vocabulary", ids[4]), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["books_in_language"], 5);
    let characteristic: Vec<&str> = v["characteristic"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    assert!(characteristic.iter().any(|w| w.starts_with("zorbal")), "{characteristic:?}");
    assert!(!characteristic.contains(&"hundo"), "in every book: {characteristic:?}");
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{}/vocabulary", ids[0]), Some(&alice), None).await;
    assert_eq!(v["characteristic"], json!([]), "nothing of its own: {v}");
}

#[tokio::test]
async fn search_finds_a_word_by_form_and_by_meaning() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let text = "Ayer Juan hizo la cena. Luego quiso ir a la casa de su madre, y fue a pie. \
                Hoy hace frío y Juan quiere dormir. La casa es grande. "
        .repeat(60);
    let up = upload(&app, &alice, &[("cena.epub", book("Cena", "es-ES", &text))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    let search = |q: &'static str| {
        let (app, alice) = (app.clone(), alice.clone());
        async move {
            let (status, m) = send(&app, Method::GET, &format!("/api/vocabulary/search?language=es&q={q}"), Some(&alice), None).await;
            assert_eq!(status, StatusCode::OK, "{m}");
            m["matches"].as_array().unwrap().clone()
        }
    };
    // By a form the book writes: "hizo" is "hacer".
    let found = search("Hizo").await;
    assert_eq!(found[0]["word"], "hacer", "{found:?}");
    assert_eq!(found[0]["books"], 1);
    assert_eq!(found[0]["book_ids"], json!([id]));
    assert_eq!(found[0]["known"], false);
    assert!(found[0]["english"].as_str().unwrap().contains("to do"));
    // By the English: "dinner" is "cena".
    let found = search("dinner").await;
    assert!(found.iter().any(|w| w["word"] == "cena"), "{found:?}");
    // The exact word first, then the commonest; a known word says so.
    let found = search("a").await;
    assert_eq!(found[0]["word"], "a", "{found:?}");
    assert!(found.len() > 2);
    send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "es", "stem": "casa", "known": true }))).await;
    let found = search("casa").await;
    assert_eq!(found[0]["word"], "casa");
    assert_eq!(found[0]["known"], true);
    // Nothing, and the wildcards of LIKE are letters here.
    assert!(search("zzzzq").await.is_empty());
    assert!(search("%25").await.is_empty());
    assert_eq!(search("").await, Vec::<serde_json::Value>::new());
}


#[tokio::test]
async fn a_pack_for_a_language_without_a_stemmer_gives_a_study_list_and_a_new_pack_recounts() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    // An operator's pack for "xx", a language Legejo has no stemmer for,
    // in the data directory before anything asks for it.
    let dir = state.data_dir.join("languages/xx");
    crate::langpack::write_xx(&dir, "to run", "1");
    // "flib" is a box and a form of "blorp"; the book blorps more than it boxes.
    let text = "Zams blorped ko flib. Zam blorping ko zams, flib blorped. Ko zam! ".repeat(60);
    let up = upload(&app, &alice, &[("xx.epub", book("Xx", "xx", &text))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    let (status, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["language"], "xx");
    assert!(v["words"].as_i64().unwrap() > 0, "{v}");
    let study = v["study"].as_array().unwrap_or_else(|| panic!("no study list: {v}"));
    let blorp = study.iter().find(|w| w["stem"] == "blorp").unwrap_or_else(|| panic!("{study:?}"));
    assert_eq!(blorp["english"], "to run");
    assert_eq!(blorp["class"], "verb");
    let forms: Vec<&str> = blorp["forms"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    assert!(forms.contains(&"blorped") && forms.contains(&"flib"), "{forms:?}");
    assert!(!study.iter().any(|w| w["stem"] == "flib"), "{study:?}");
    assert_eq!(study.iter().find(|w| w["stem"] == "ko").unwrap()["class"], "other");
    assert_eq!(v["study_source"], "the tests");
    let (status, _) = send(&app, Method::PUT, "/api/vocabulary/known", Some(&alice), Some(json!({ "language": "xx", "stem": "blorp", "known": true }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, langs) = send(&app, Method::GET, "/api/vocabulary/languages", Some(&alice), None).await;
    assert_eq!(langs, json!([{ "language": "xx", "books": 1, "known": 1 }]));
    let (status, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=xx", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{o}");
    assert_eq!(o["pack"]["name"], "Xxish", "{o}");
    assert_eq!(o["pack"]["built_in"], false);
    assert_eq!(o["source"], "the tests");
    assert_eq!(o["books"].as_array().unwrap().len(), 1);
    let (status, w) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary/word?w=Blorping,"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{w}");
    assert_eq!(w["word"], "blorp");
    assert_eq!(w["english"], "to run");
    assert_eq!(w["known"], true);
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 0, "counted with the pack there is");

    // A new pack (another gloss, another version) is noticed once reloaded:
    // the book is counted again, and the list shows the new English.
    crate::langpack::write_xx(&dir, "to sprint", "2");
    state.packs.reload("xx").unwrap();
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 1);
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 0);
    let (_, v) = send(&app, Method::GET, &format!("/api/books/{id}/vocabulary"), Some(&alice), None).await;
    assert_eq!(v["study"].as_array().unwrap().iter().find(|w| w["stem"] == "blorp").unwrap()["english"], "to sprint", "{v}");
    // A row counted without a pack, by this version, is counted again now
    // that the language has one.
    sqlx::query("UPDATE vocabulary SET pack = ''").execute(&db).await.unwrap();
    assert_eq!(vocabulary::count_pending(&state, 10).await.unwrap(), 1);
    let (pack,): (String,) = sqlx::query_as("SELECT pack FROM vocabulary").fetch_one(&db).await.unwrap();
    assert!(pack.starts_with("xx:2:"), "{pack}");
}

#[tokio::test]
async fn books_counted_before_the_word_table_are_put_in_it_in_the_background() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let a = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let a = a["added"][0]["id"].as_i64().unwrap();
    send(&app, Method::GET, &format!("/api/books/{a}/vocabulary"), Some(&alice), None).await;
    // As a version without the table would have left it.
    sqlx::query("DELETE FROM vocabulary_words").execute(&db).await.unwrap();
    sqlx::query("DELETE FROM vocabulary_indexed").execute(&db).await.unwrap();
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert_eq!(o["next"], json!([]), "{o}");
    assert_eq!(crate::vocabulary::index_pending(&state, 20).await.unwrap(), 1);
    assert_eq!(crate::vocabulary::index_pending(&state, 20).await.unwrap(), 0);
    let (_, o) = send(&app, Method::GET, "/api/vocabulary/overview?language=eo", Some(&alice), None).await;
    assert!(!o["next"].as_array().unwrap().is_empty(), "{o}");
    assert!(o["books"][0]["share"].as_f64().unwrap() == 0.0);
}
