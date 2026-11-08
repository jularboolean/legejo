//! Admin → Languages: listing, importing and removing language packs.

use crate::testutil::{add_book, add_user, send, test_app};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use std::io::Write;
use tower::ServiceExt;

/// The files of the test pack "xx" (langpack::write_xx), by name.
fn xx_files(gloss: &str, version: &str) -> Vec<(String, Vec<u8>)> {
    let dir = std::env::temp_dir().join(format!("legejo-xxzip-{}", crate::books::new_uuid()));
    crate::langpack::write_xx(&dir, gloss, version);
    let files = ["pack.json", "lemmas.tsv", "forms.tsv"].iter().map(|n| (n.to_string(), std::fs::read(dir.join(n)).unwrap())).collect();
    std::fs::remove_dir_all(&dir).unwrap();
    files
}

/// A zip of the given entries; a name ending in "/" is a directory.
fn zip_of(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        if name.ends_with('/') {
            z.add_directory(name.as_str(), options).unwrap();
        } else {
            z.start_file(name.as_str(), options).unwrap();
            z.write_all(data).unwrap();
        }
    }
    z.finish().unwrap().into_inner()
}

async fn import(app: &Router, cookie: &str, zip: &[u8]) -> (StatusCode, Value) {
    let boundary = "legejo-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"xx.zip\"\r\nContent-Type: application/zip\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(zip);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/languages")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn find<'a>(list: &'a Value, code: &str) -> Option<&'a Value> {
    list["packs"].as_array().unwrap().iter().find(|p| p["code"] == code)
}

/// The leftovers of an import or removal in the languages directory.
fn hidden_dirs(state: &crate::AppState) -> Vec<String> {
    std::fs::read_dir(state.packs.dir())
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|n| n.starts_with('.')).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn an_admin_imports_replaces_and_removes_a_pack() {
    let (app, db, state) = test_app().await;
    let (admin_id, admin) = add_user(&db, "ada").await;
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin_id).execute(&db).await.unwrap();
    let (bob_id, bob) = add_user(&db, "bob").await;
    let (book, _) = add_book(&db, bob_id, "Xx book").await;
    sqlx::query("UPDATE books SET language = 'xx' WHERE id = ?").bind(book).execute(&db).await.unwrap();

    // Only for admins.
    let (status, _) = send(&app, Method::GET, "/api/admin/languages", Some(&bob), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = import(&app, &bob, &zip_of(&xx_files("to run", "1"))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&bob), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The built-in packs, sorted by name.
    let (status, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["catalogue"], "https://legejo.se/languages");
    let codes: Vec<&str> = list["packs"].as_array().unwrap().iter().map(|p| p["code"].as_str().unwrap()).collect();
    assert_eq!(codes.len(), 3, "{list}");
    let es = find(&list, "es").unwrap();
    assert_eq!(es["built_in"], true);
    assert_eq!(es["replaces_built_in"], false);
    assert!(es["lemmas"].as_u64().unwrap() > 1000 && es["forms"].as_u64().unwrap() > 1000, "{es}");
    assert!(find(&list, "xx").is_none());

    // Imported: listed with its counts, and in use.
    let (status, entry) = import(&app, &admin, &zip_of(&xx_files("to run", "1"))).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["code"], "xx");
    assert_eq!(entry["name"], "Xxish");
    assert_eq!(entry["version"], "1");
    assert_eq!(entry["built_in"], false);
    assert_eq!(entry["replaces_built_in"], false);
    assert_eq!(entry["lemmas"], 4);
    assert_eq!(entry["forms"], 7);
    assert_eq!(entry["books"], 1);
    assert_eq!(state.packs.get("xx").unwrap().find("blorp").unwrap().english, "to run");
    let (_, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(find(&list, "xx").unwrap()["lemmas"], 4);
    assert!(hidden_dirs(&state).is_empty(), "{:?}", hidden_dirs(&state));

    // Again, a newer version in a folder: it replaces the first.
    let wrapped: Vec<(String, Vec<u8>)> = std::iter::once(("xx/".to_string(), Vec::new()))
        .chain(xx_files("to sprint", "2").into_iter().map(|(n, d)| (format!("xx/{n}"), d)))
        .chain([("xx/README.txt".to_string(), b"CC0".to_vec()), ("__MACOSX/xx/._pack.json".to_string(), b"x".to_vec())])
        .collect();
    let (status, entry) = import(&app, &admin, &zip_of(&wrapped)).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["version"], "2");
    let xx = state.packs.get("xx").unwrap();
    assert_eq!(xx.find("blorp").unwrap().english, "to sprint");
    assert!(xx.id().starts_with("xx:2:"));
    assert!(state.packs.dir().join("xx/README.txt").is_file());
    assert!(hidden_dirs(&state).is_empty(), "{:?}", hidden_dirs(&state));

    // Bad zips: refused with a reason, and the pack there is stays.
    let good = xx_files("to fly", "3");
    let with = |extra: (&str, &[u8])| {
        let mut files = good.clone();
        files.push((extra.0.to_string(), extra.1.to_vec()));
        zip_of(&files)
    };
    let without = |name: &str| zip_of(&good.iter().filter(|(n, _)| n != name).cloned().collect::<Vec<_>>());
    let replace = |name: &str, data: &[u8]| {
        zip_of(&good.iter().map(|(n, d)| if n == name { (n.clone(), data.to_vec()) } else { (n.clone(), d.clone()) }).collect::<Vec<_>>())
    };
    let pack_json = |language: &str, format: u32| {
        format!(r#"{{"format": {format}, "language": "{language}", "name": "N", "version": "3", "source": "", "license": "", "attribution": ""}}"#)
    };
    let mixed: Vec<(String, Vec<u8>)> = good.iter().enumerate().map(|(i, (n, d))| (if i == 0 { format!("a/{n}") } else { n.clone() }, d.clone())).collect();
    let deep: Vec<(String, Vec<u8>)> = good.iter().map(|(n, d)| (format!("a/b/{n}"), d.clone())).collect();
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("a path outside", with(("../evil", b"x")), "outside"),
        ("an extra file", with(("notes.md", b"x")), "holds only"),
        ("no forms.tsv", without("forms.tsv"), "no forms.tsv"),
        ("Esperanto", replace("pack.json", pack_json("eo", 1).as_bytes()), "Esperanto"),
        ("not a code", replace("pack.json", pack_json("Xx", 1).as_bytes()), "not a language code"),
        ("bad JSON", replace("pack.json", b"{ not json"), "pack.json"),
        ("an unknown format", replace("pack.json", pack_json("xx", 2).as_bytes()), "cannot be loaded"),
        ("no forms that fit", replace("forms.tsv", b"zz\tnothing\n"), "cannot be loaded"),
        ("not UTF-8", replace("lemmas.tsv", &[0xff, 0xfe, b'\t', b'x']), "cannot be loaded"),
        ("root and folder", zip_of(&mixed), "one folder"),
        ("a folder in a folder", zip_of(&deep), "folders inside"),
        ("not a zip", b"PK nothing".to_vec(), "not a zip"),
    ];
    for (what, zip, says) in cases {
        let (status, body) = import(&app, &admin, &zip).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{what}: {body}");
        assert!(body["error"].as_str().unwrap_or("").contains(says), "{what}: {body}");
        assert_eq!(state.packs.get("xx").unwrap().find("blorp").unwrap().english, "to sprint", "{what}");
        assert!(hidden_dirs(&state).is_empty(), "{what}: {:?}", hidden_dirs(&state));
    }
    assert_eq!(crate::langpack::check_dir(&state.packs.dir().join("xx"), "xx").unwrap().find("blorp").unwrap().english, "to sprint");

    // A pack for a built-in language replaces it until removed.
    let es_files: Vec<(String, Vec<u8>)> =
        good.iter().map(|(n, d)| if n == "pack.json" { (n.clone(), pack_json("es", 1).into_bytes()) } else { (n.clone(), d.clone()) }).collect();
    let (status, entry) = import(&app, &admin, &zip_of(&es_files)).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["replaces_built_in"], true);
    assert!(state.packs.get("es").unwrap().lemmas_of("hizo").is_empty());
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let es = state.packs.get("es").unwrap();
    assert!(es.meta.built_in && !es.lemmas_of("hizo").is_empty());

    // A built-in pack cannot be removed; one that is not there is not found.
    let (status, body) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&admin), None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/sw", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/..", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Removed: gone.
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/xx", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(state.packs.get("xx").is_none());
    assert!(!state.packs.dir().join("xx").exists());
    let (_, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert!(find(&list, "xx").is_none());
    assert!(hidden_dirs(&state).is_empty(), "{:?}", hidden_dirs(&state));

    // Logged.
    let actions: Vec<String> = sqlx::query_scalar("SELECT action FROM activity_log WHERE action LIKE 'admin.language%' ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(actions, ["admin.language_imported", "admin.language_imported", "admin.language_imported", "admin.language_removed", "admin.language_removed"]);

    // An unknown path under it is a 404.
    let (status, _) = send(&app, Method::GET, "/api/admin/languages/xx/other", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
