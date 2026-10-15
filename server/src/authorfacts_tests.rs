//! Who an author was, from Wikidata: picking the one certain person out of
//! what the search finds, and the line the library gets from what is kept.

#![cfg(test)]

use crate::authorfacts::{choose, key_of, Facts};
use crate::testutil::{add_user, send, test_app};
use axum::http::{Method, StatusCode};
use serde_json::{json, Value};

/// An entity as wbgetentities gives it, cut to what matters here.
fn entity(id: &str, labels: &[(&str, &str)], humans: bool, occupations: &[&str], born: Option<&str>) -> Value {
    let claim = |id: &str| json!({ "mainsnak": { "datavalue": { "value": { "id": id } } } });
    let mut claims = json!({
        "P31": [claim(if humans { "Q5" } else { "Q7725634" })],
        "P106": occupations.iter().map(|o| claim(o)).collect::<Vec<_>>(),
    });
    if let Some(born) = born {
        claims["P569"] = json!([{ "mainsnak": { "datavalue": { "value": { "time": born } } } }]);
    }
    json!({
        "id": id,
        "labels": labels.iter().map(|(l, v)| (l.to_string(), json!({ "value": v }))).collect::<serde_json::Map<_, _>>(),
        "descriptions": { "sv": { "value": "svensk författare" }, "en": { "value": "Swedish writer" } },
        "claims": claims,
        "sitelinks": { "svwiki": { "title": "Selma Lagerlöf" }, "enwiki": { "title": "Selma Lagerlöf" }, "commonswiki": { "title": "x" } },
    })
}

/// The same, found in `n` Wikipedias.
fn known(mut e: Value, n: usize) -> Value {
    for i in 0..n {
        e["sitelinks"][format!("x{i}wiki")] = json!({ "title": "t" });
    }
    e
}

fn entities(list: Vec<Value>) -> Value {
    json!({ "entities": list.into_iter().map(|e| (e["id"].as_str().unwrap().to_string(), e)).collect::<serde_json::Map<_, _>>() })
}

#[test]
fn a_name_is_folded_as_the_web_does_it() {
    assert_eq!(key_of("Selma Lagerlöf"), "selma lagerlof");
    assert_eq!(key_of("  Lagerlöf,  Selma "), "lagerlof selma");
    assert_eq!(key_of("Philip K. Dick"), "philip k dick");
    assert_eq!(key_of("Bolesław Prus"), "bolesław prus", "ł is a letter of its own, not a mark on l");
    assert_eq!(key_of("..."), "");
}

#[test]
fn the_one_writer_with_the_name_is_the_author() {
    let found = entities(vec![
        entity("Q44519", &[("sv", "Selma Lagerlöf"), ("en", "Selma Lagerlöf")], true, &["Q36180", "Q6625963"], Some("+1858-11-20T00:00:00Z")),
        // A ship named after her: not a person, so no rival.
        entity("Q99", &[("sv", "Selma Lagerlöf")], false, &[], None),
    ]);
    let facts = choose(&found, &key_of("Selma Lagerlöf")).unwrap();
    assert_eq!(facts.qid, "Q44519");
    assert_eq!(facts.born, Some(1858));
    assert_eq!(facts.died, None);
    assert_eq!(facts.descriptions["sv"], "svensk författare");
    assert_eq!(facts.wiki.get("sv").map(String::as_str), Some("Selma Lagerlöf"));
    assert!(!facts.wiki.contains_key("commons"), "only the Wikipedias of the interface's languages");
}

#[test]
fn two_writers_of_the_same_name_give_no_one() {
    let found = entities(vec![
        entity("Q1", &[("en", "John Smith")], true, &["Q36180"], None),
        entity("Q2", &[("en", "John Smith")], true, &["Q49757"], None),
    ]);
    assert_eq!(choose(&found, "john smith"), None);
    // Nor does a writer whose names are all other than the author's.
    let found = entities(vec![entity("Q3", &[("en", "Johan Smith")], true, &["Q36180"], None)]);
    assert_eq!(choose(&found, "john smith"), None);
    // One writer among people of the same name who did other things is no
    // proof either: the author may be one of whom Wikidata knows nothing.
    let found = entities(vec![
        entity("Q4", &[("en", "John Smith")], true, &["Q4964182"], None),
        entity("Q5", &[("en", "John Smith")], true, &["Q82955"], None),
    ]);
    assert_eq!(choose(&found, "john smith"), None);
}

#[test]
fn an_author_far_better_known_than_others_of_the_name_is_the_one() {
    // Mary Shelley, and seven others of her name of whom no Wikipedia writes.
    let mut found = vec![known(entity("Q47152", &[("en", "Mary Shelley")], true, &["Q6625963"], None), 130)];
    for i in 0..7 {
        found.push(entity(&format!("Q{i}"), &[("en", "Mary Shelley")], true, &[], None));
    }
    assert_eq!(choose(&entities(found), "mary shelley").unwrap().qid, "Q47152");

    // William Gibson the novelist, and William Gibson the playwright, less known.
    let found = entities(vec![
        known(entity("Q188987", &[("en", "William Gibson")], true, &["Q6625963"], None), 65),
        known(entity("Q980844", &[("en", "William Gibson")], true, &["Q214917"], None), 17),
    ]);
    assert_eq!(choose(&found, "william gibson").unwrap().qid, "Q188987");

    // Not when the other is nearly as well known, nor when the best known
    // of the name is not a writer, nor when no one is much known.
    let found = entities(vec![
        known(entity("Q1", &[("en", "Anna Berg")], true, &["Q36180"], None), 20),
        known(entity("Q2", &[("en", "Anna Berg")], true, &["Q82955"], None), 9),
    ]);
    assert_eq!(choose(&found, "anna berg"), None);
    let found = entities(vec![
        known(entity("Q3", &[("en", "John Smith")], true, &["Q82955"], None), 28),
        known(entity("Q4", &[("en", "John Smith")], true, &["Q4964182"], None), 8),
    ]);
    assert_eq!(choose(&found, "john smith"), None);
    let found = entities(vec![
        known(entity("Q5", &[("en", "Per Ek")], true, &["Q36180"], None), 6),
        entity("Q6", &[("en", "Per Ek")], true, &[], None),
    ]);
    assert_eq!(choose(&found, "per ek"), None);
}

#[test]
fn a_year_before_the_common_era_is_negative() {
    let found = entities(vec![entity("Q41568", &[("en", "Homer")], true, &["Q49757"], Some("-0800-00-00T00:00:00Z"))]);
    assert_eq!(choose(&found, "homer").unwrap().born, Some(-800));
}

async fn keep(db: &sqlx::AnyPool, key: &str, state: &str, facts: Option<&Facts>, fetched_at: &str) {
    sqlx::query("INSERT INTO author_facts (author_key, name, state, qid, facts, fetched_at) VALUES (?, 'Selma Lagerlöf', ?, ?, ?, ?)")
        .bind(key)
        .bind(state)
        .bind(facts.map(|f| f.qid.clone()))
        .bind(facts.map(|f| serde_json::to_string(f).unwrap()))
        .bind(fetched_at)
        .execute(db)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_library_gets_a_line_only_for_an_author_found_for_certain() {
    let (app, db, _) = test_app().await;
    let (_, a) = add_user(&db, "alice").await;
    let (_, b) = add_user(&db, "bob").await;
    let url = "/api/authors/facts?name=Selma%20Lagerl%C3%B6f&lang=sv";

    // Off until an admin turns it on.
    let (status, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "off");
    sqlx::query("INSERT INTO settings (key, value) VALUES ('author_facts_enabled', 'true')").execute(&db).await.unwrap();

    // Not looked up yet: nothing to show this time.
    let (_, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(v["state"], "pending");

    // Found: the line in the user's language, Wikipedia included.
    let facts = Facts {
        qid: "Q44519".into(),
        born: Some(1858),
        died: Some(1940),
        labels: [("sv".to_string(), "Selma Lagerlöf".to_string())].into(),
        descriptions: [("sv".to_string(), "svensk författare".to_string()), ("en".to_string(), "Swedish writer".to_string())].into(),
        wiki: [("sv".to_string(), "Selma Lagerlöf".to_string())].into(),
    };
    keep(&db, "selma lagerlof", "ok", Some(&facts), &crate::db::now_ts()).await;
    let (_, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(v["state"], "ok", "{v}");
    assert_eq!((v["born"].as_i64(), v["died"].as_i64()), (Some(1858), Some(1940)));
    assert_eq!(v["description"], "svensk författare");
    assert_eq!(v["wikipedia"]["url"], "https://sv.wikipedia.org/wiki/Selma_Lagerl%C3%B6f");
    // In another language: its own description, and Swedish Wikipedia only if
    // there is nothing in English.
    let (_, v) = send(&app, Method::GET, "/api/authors/facts?name=Selma%20Lagerl%C3%B6f&lang=en", Some(&a), None).await;
    assert_eq!(v["description"], "Swedish writer");
    assert!(v["wikipedia"].is_null(), "no English article was kept: {v}");

    // Old, it is still shown (and looked up again in the background).
    sqlx::query("UPDATE author_facts SET fetched_at = '2020-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
    let (_, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(v["state"], "ok");

    // The wrong person, says Alice: not for her again, still for Bob.
    let (status, _) = send(&app, Method::POST, "/api/authors/facts/hidden", Some(&a), Some(json!({ "name": "Selma Lagerlöf" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(v["state"], "hidden");
    let (_, v) = send(&app, Method::GET, url, Some(&b), None).await;
    assert_eq!(v["state"], "ok");
    // Undone: hers again.
    let (status, _) = send(&app, Method::DELETE, "/api/authors/facts/hidden", Some(&a), Some(json!({ "name": "Selma Lagerlöf" }))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, url, Some(&a), None).await;
    assert_eq!(v["state"], "ok");

    // No certain match, or Wikidata not answering: no line.
    keep(&db, "john smith", "none", None, &crate::db::now_ts()).await;
    let (_, v) = send(&app, Method::GET, "/api/authors/facts?name=John%20Smith", Some(&a), None).await;
    assert_eq!(v["state"], "none");
    keep(&db, "jane smith", "error", None, &crate::db::now_ts()).await;
    let (_, v) = send(&app, Method::GET, "/api/authors/facts?name=Jane%20Smith", Some(&a), None).await;
    assert_eq!(v["state"], "pending");

    // Not without a login, and not without a name.
    let (status, _) = send(&app, Method::GET, url, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(&app, Method::GET, "/api/authors/facts?name=...", Some(&a), None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
