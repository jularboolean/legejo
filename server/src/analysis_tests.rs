//! A book's analysis over HTTP: computed when first asked for and kept once
//! per file, the concordance read from the text, and a word's forms.

use crate::library_tests::upload;
use crate::testutil::{add_user, send, test_app};
use crate::vocabulary_tests::book;
use axum::http::{Method, StatusCode};

fn esperanto() -> String {
    let mut text = String::new();
    for i in 0..300 {
        text.push_str("Poste la hundo diris al Petro ke la grandaj katoj bone dormas. ");
        if i % 5 == 0 {
            text.push_str("«Mi venos morgaŭ», respondis Petro al la hundo. ");
        }
    }
    text
}

async fn kept(db: &sqlx::AnyPool) -> Vec<(String, String)> {
    sqlx::query_as("SELECT language, data FROM book_analysis ORDER BY language").fetch_all(db).await.unwrap()
}

#[tokio::test]
async fn a_book_is_analysed_when_asked_for_and_kept_once_per_file() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let (_, bob) = add_user(&db, "bob").await;
    let up = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));
    let up = upload(&app, &bob, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let bobs = up["added"][0]["id"].as_i64().unwrap();
    assert!(kept(&db).await.is_empty(), "nothing is analysed before it is asked for");

    let (status, a) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["language"], "eo");
    assert_eq!(a["style"]["sentences"], 360);
    assert_eq!(a["style"]["words"], 300 * 12 + 60 * 8);
    assert!(a["style"]["spoken"].as_f64().unwrap() > 0.04, "{}", a["style"]);
    assert_eq!(a["style"]["own"], serde_json::Value::Null);
    assert_eq!(a["people"]["names"][0]["name"], "Petro");
    assert_eq!(a["people"]["names"][0]["count"], 360);
    let verbs = a["grammar"]["groups"].as_array().unwrap().iter().find(|g| g["key"] == "verb").unwrap();
    assert_eq!(verbs["of"], 300 * 2 + 60 * 2);
    assert_eq!(a["grammar"]["source"], "endings");

    // Bob's copy is the same file: the analysis there is, not another.
    let (status, b) = send(&app, Method::GET, &format!("/api/books/{bobs}/analysis"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(a, b);
    assert_eq!(kept(&db).await.len(), 1);
    // Nobody reads another reader's book this way.
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/analysis/concordance?q=hundo"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Once no book has the file, its analysis goes.
    for (book, cookie) in [(id, &alice), (bobs, &bob)] {
        assert_eq!(crate::analysis::drop_orphans(&state).await.unwrap(), 0);
        let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{book}"), Some(cookie), None).await;
        assert!(status.is_success(), "{status}");
    }
    assert_eq!(crate::analysis::drop_orphans(&state).await.unwrap(), 1);
    assert!(kept(&db).await.is_empty());
}

#[tokio::test]
async fn a_short_book_has_no_analysis_and_is_not_read_again() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let up = upload(&app, &alice, &[("kort.epub", book("Kort", "sv", "En kort bok om en hund. Den är slut nu."))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap();
    for _ in 0..2 {
        let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), Some(&alice), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    assert_eq!(kept(&db).await, [("sv".to_string(), "null".to_string())]);
    let (status, _) = send(&app, Method::GET, "/api/books/999999/analysis", Some(&alice), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_concordance_and_a_words_forms_are_answered() {
    let (app, db, _state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let up = upload(&app, &alice, &[("hundo.epub", book("Hundo", "eo", &esperanto()))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap();

    let (status, c) = send(&app, Method::GET, &format!("/api/books/{id}/analysis/concordance?q=kato"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{c}");
    assert_eq!(c["total"], 300);
    assert_eq!(c["lines"].as_array().unwrap().len(), 300);
    assert_eq!(c["lines"][0]["hit"], "katoj");
    assert!(c["lines"][0]["left"].as_str().unwrap().ends_with("ke la grandaj"), "{}", c["lines"][0]);
    assert_eq!(c["forms"][0][0], "katoj");
    // The form as written is not in the book; a phrase is.
    let (_, c) = send(&app, Method::GET, &format!("/api/books/{id}/analysis/concordance?q=kato&exact=true"), Some(&alice), None).await;
    assert_eq!(c["total"], 0);
    let (_, c) = send(&app, Method::GET, &format!("/api/books/{id}/analysis/concordance?q=mi%20venos"), Some(&alice), None).await;
    assert_eq!(c["total"], 60);
    for q in ["", "%20", &"a".repeat(81)] {
        let (status, _) = send(&app, Method::GET, &format!("/api/books/{id}/analysis/concordance?q={q}"), Some(&alice), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{q}");
    }

    let (status, f) = send(&app, Method::GET, "/api/analysis/forms?language=eo&w=katojn", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{f}");
    assert_eq!(f["word"], "kato");
    assert_eq!(f["tables"][0]["rows"][3]["form"], "katojn");
    assert_eq!(f["tables"][0]["rows"][3]["this"], true);
    for query in ["language=eo&w=kaj", "language=es&w=casa"] {
        let (status, _) = send(&app, Method::GET, &format!("/api/analysis/forms?{query}"), Some(&alice), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{query}");
    }
    let (status, _) = send(&app, Method::GET, "/api/analysis/forms?language=eo&w=katojn", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_pack_with_extras_gives_grammar_a_level_and_tables_of_forms() {
    let (app, db, state) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let dir = state.data_dir.join("languages/xx");
    crate::langpack::write_xx(&dir, "to run", "1");
    state.packs.reload("xx").unwrap();
    let text = "Ko blorped zam flib ko zams. Zam blorping ko zam ko. ".repeat(60);
    let up = upload(&app, &alice, &[("xx.epub", book("Xx", "xx", &text))], "").await;
    let id = up["added"][0]["id"].as_i64().unwrap_or_else(|| panic!("{up}"));

    // Without extras: the style and the phrases, no grammar and no level.
    let (status, a) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["language"], "xx");
    assert_eq!((&a["grammar"], &a["reach"], &a["forms"]), (&serde_json::Value::Null, &serde_json::Value::Null, &serde_json::json!(false)));
    let (status, _) = send(&app, Method::GET, "/api/analysis/forms?language=xx&w=blorped", Some(&alice), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The extras come: the book is analysed again, its words not counted again.
    crate::langpack::write_xx_extras(&dir);
    state.packs.reload("xx").unwrap();
    let (status, a) = send(&app, Method::GET, &format!("/api/books/{id}/analysis"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["grammar"]["source"], "pack");
    let tense = a["grammar"]["groups"].as_array().unwrap().iter().find(|g| g["key"] == "tense").unwrap_or_else(|| panic!("{a}"));
    let past = tense["items"].as_array().unwrap().iter().find(|i| i["tag"] == "past").unwrap();
    // "blorped" is the past; "flib" may be it, or the noun.
    assert_eq!((past["count"].as_i64(), past["possible"].as_i64()), (Some(60), Some(60)));
    assert_eq!(a["reach"]["cefr"], "A1");
    assert_eq!(a["reach"]["source"], "A word list, CC0");
    assert_eq!(a["forms"], true);
    assert_eq!(kept(&db).await.len(), 1);

    let (status, f) = send(&app, Method::GET, "/api/analysis/forms?language=xx&w=blorped", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{f}");
    assert_eq!(f["word"], "blorp");
    let tables = f["tables"].as_array().unwrap();
    assert!(tables.iter().any(|t| t["title"] == serde_json::json!(["past"]) && t["rows"].as_array().unwrap().iter().any(|r| r["form"] == "blorped" && r["this"] == true)), "{f}");
}
