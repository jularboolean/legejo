//! Admin → Languages: listing, importing, installing from the catalogue and
//! removing language packs; the catalogue played by a server of the test's
//! own.

use crate::testutil::{add_book, add_user, send, test_app};
use crate::AppState;
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
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

    // No pack yet; Esperanto is always there, apart.
    let (status, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["catalogue"], "https://legejo.se/languages");
    assert_eq!(list["catalogue_url"], Value::Null, "the tests have no catalogue");
    assert_eq!(list["available"], serde_json::json!([]));
    assert_eq!(list["packs"], serde_json::json!([]));
    assert_eq!(list["esperanto"]["books"], 0);

    // Imported: listed with its counts, and in use.
    let (status, entry) = import(&app, &admin, &zip_of(&xx_files("to run", "1"))).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["code"], "xx");
    assert_eq!(entry["name"], "Xxish");
    assert_eq!(entry["version"], "1");
    assert!(entry.get("built_in").is_none(), "{entry}");
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

    // A pack for Spanish is a pack like any other: removed, it is gone.
    let es_files: Vec<(String, Vec<u8>)> =
        good.iter().map(|(n, d)| if n == "pack.json" { (n.clone(), pack_json("es", 1).into_bytes()) } else { (n.clone(), d.clone()) }).collect();
    let (status, entry) = import(&app, &admin, &zip_of(&es_files)).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["code"], "es");
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(state.packs.get("es").is_none());

    // One that is not there is not found.
    let (status, body) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
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

// ------------------------------------------------------------ the catalogue

/// What the pretend catalogue serves, by path: a body, or a redirect.
#[derive(Clone)]
enum Served {
    Body(Vec<u8>),
    Redirect(String),
}

/// The pretend catalogue: its base URL, what it serves, and the requests it
/// got (path and User-Agent).
struct Catalogue {
    base: String,
    files: Arc<Mutex<HashMap<String, Served>>>,
    seen: Arc<Mutex<Vec<(String, String)>>>,
}

async fn start_catalogue() -> Catalogue {
    let files: Arc<Mutex<HashMap<String, Served>>> = Arc::default();
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let (served, noted) = (files.clone(), seen.clone());
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let (served, noted) = (served.clone(), noted.clone());
        async move {
            use axum::response::IntoResponse;
            let path = request.uri().path().to_string();
            let agent = request.headers().get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
            noted.lock().unwrap().push((path.clone(), agent));
            let found = served.lock().unwrap().get(&path).cloned();
            match found {
                Some(Served::Body(bytes)) => (StatusCode::OK, bytes).into_response(),
                Some(Served::Redirect(to)) => (StatusCode::FOUND, [(header::LOCATION, to)]).into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/languages", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Catalogue { base, files, seen }
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A catalogue entry for a zip.
fn offer(code: &str, name: &str, version: &str, zip: &[u8], sha: Option<&str>) -> Value {
    serde_json::json!({
        "code": code, "name": name, "english": format!("{name} in English"), "version": version,
        "lemmas": 4, "forms": 7, "bytes": zip.len(), "sha256": sha.map(str::to_string).unwrap_or_else(|| sha256(zip)),
        "license": "CC0", "attribution": "the tests"
    })
}

/// The test pack "xx" as a zip, said to be for `code`.
fn pack_zip(code: &str, gloss: &str, version: &str) -> Vec<u8> {
    let files: Vec<(String, Vec<u8>)> = xx_files(gloss, version)
        .into_iter()
        .map(|(n, d)| {
            if n == "pack.json" {
                let text = String::from_utf8(d).unwrap().replace(r#""language": "xx""#, &format!(r#""language": "{code}""#));
                (n, text.into_bytes())
            } else {
                (n, d)
            }
        })
        .collect();
    zip_of(&files)
}

/// An app whose catalogue is at `base` (None: off); an admin's cookie.
async fn with_catalogue(base: Option<&str>) -> (Router, sqlx::AnyPool, AppState, String) {
    let (_, db, mut state) = test_app().await;
    state.settings = Arc::new(crate::settings::Settings { languages_url: base.map(str::to_string), ..crate::settings::Settings::default() });
    let app = crate::router(state.clone());
    let (admin_id, admin) = add_user(&db, "ada").await;
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin_id).execute(&db).await.unwrap();
    (app, db, state, admin)
}

async fn book_in(db: &sqlx::AnyPool, owner: i64, title: &str, language: &str) {
    let (book, _) = add_book(db, owner, title).await;
    sqlx::query("UPDATE books SET language = ? WHERE id = ?").bind(language).bind(book).execute(db).await.unwrap();
}

fn available<'a>(list: &'a Value, code: &str) -> &'a Value {
    list["available"].as_array().unwrap().iter().find(|p| p["code"] == code).unwrap_or_else(|| panic!("{code} not in {list}"))
}

#[tokio::test]
async fn an_admin_installs_a_pack_from_the_catalogue_with_one_click() {
    let catalogue = start_catalogue().await;
    let xx = pack_zip("xx", "to run", "2");
    let yy = pack_zip("yy", "to run", "1");
    let qq = pack_zip("qq", "to run", "1");
    let index = serde_json::json!({ "format": 1, "built": "2026-11-08", "packs": [
        offer("xx", "Xxish", "2", &xx, None),
        offer("yy", "Yyish", "1", &yy, Some(&"0".repeat(64))),
        offer("qq", "Qqish", "1", &qq, None),
        offer("eo", "Esperanto", "1", &qq, None),
    ]});
    {
        let mut files = catalogue.files.lock().unwrap();
        files.insert("/languages/index.json".into(), Served::Body(index.to_string().into_bytes()));
        // As legejo.se does: a redirect to where the file is.
        files.insert("/languages/xx.zip".into(), Served::Redirect("/files/xx.zip".into()));
        files.insert("/files/xx.zip".into(), Served::Body(xx.clone()));
        files.insert("/languages/yy.zip".into(), Served::Body(yy.clone()));
    }
    let (app, db, state, admin) = with_catalogue(Some(&catalogue.base)).await;
    let (bob, _) = add_user(&db, "bob").await;
    book_in(&db, bob, "Xx 1", "xx").await;
    book_in(&db, bob, "Xx 2", "xx").await;
    book_in(&db, bob, "Qq 1", "qq").await;
    book_in(&db, bob, "Eo 1", "eo").await;

    // The catalogue's packs, with the books on the instance; Esperanto apart.
    let (status, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["catalogue_url"], catalogue.base);
    assert_eq!(list["catalogue_error"], Value::Null);
    assert_eq!(list["esperanto"]["books"], 1);
    assert_eq!(list["available"].as_array().unwrap().len(), 3, "never a pack for Esperanto: {list}");
    let x = available(&list, "xx");
    assert_eq!(x["name"], "Xxish");
    assert_eq!(x["english"], "Xxish in English");
    assert_eq!(x["books"], 2);
    assert_eq!(x["installed_version"], Value::Null);
    assert_eq!(x["update"], false);
    assert_eq!(list["suggested"], serde_json::json!(["xx", "qq"]), "most books first");

    // Installed: downloaded through the redirect, as Legejo.
    let (status, entry) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "xx" }))).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["code"], "xx");
    assert_eq!(entry["version"], "2");
    assert_eq!(entry["books"], 2);
    assert_eq!(state.packs.get("xx").unwrap().find("blorp").unwrap().english, "to run");
    let seen = catalogue.seen.lock().unwrap().clone();
    assert!(seen.iter().any(|(p, _)| p == "/files/xx.zip"), "{seen:?}");
    assert!(seen.iter().all(|(_, agent)| agent.starts_with("Legejo/") && agent.ends_with("(+https://legejo.se)")), "{seen:?}");
    let (_, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(available(&list, "xx")["installed_version"], "2");
    assert_eq!(available(&list, "xx")["update"], false);
    assert_eq!(list["suggested"], serde_json::json!(["qq"]));

    // An older pack imported by hand: the catalogue's is an update.
    let (status, _) = import(&app, &admin, &zip_of(&xx_files("to walk", "1"))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(available(&list, "xx")["installed_version"], "1");
    assert_eq!(available(&list, "xx")["update"], true);
    let (status, entry) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "xx" }))).await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert_eq!(entry["version"], "2");

    // A download that is not what the catalogue says: 502, nothing installed.
    let (status, body) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "yy" }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("sha256"), "{body}");
    assert!(state.packs.get("yy").is_none());
    assert!(!state.packs.dir().join("yy").exists());
    assert!(hidden_dirs(&state).is_empty(), "{:?}", hidden_dirs(&state));

    // In the catalogue, but the zip is not there: 502.
    let (status, body) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "qq" }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body["error"].as_str().unwrap().contains("404"), "{body}");

    // Not in the catalogue, Esperanto, not a code: 404.
    for code in ["zz", "eo", "../x"] {
        let (status, body) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": code }))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{code}: {body}");
    }

    // Only for admins.
    let (_, bob_cookie) = add_user(&db, "carl").await;
    let (status, _) = send(&app, Method::POST, "/api/admin/languages/install", Some(&bob_cookie), Some(serde_json::json!({ "code": "xx" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let actions: Vec<String> = sqlx::query_scalar("SELECT action FROM activity_log WHERE action LIKE 'admin.language%' ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(actions, ["admin.language_installed", "admin.language_imported", "admin.language_installed"]);
}

#[tokio::test]
async fn the_list_works_without_the_catalogue() {
    // Out of reach: the installed packs still, and why there is no more.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let nowhere = format!("http://{}/languages", listener.local_addr().unwrap());
    drop(listener);
    let (app, _, state, admin) = with_catalogue(Some(&nowhere)).await;
    crate::langpack::write_xx(&state.packs.dir().join("xx"), "to run", "1");
    let (status, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert_eq!(list["available"], serde_json::json!([]));
    assert!(list["catalogue_error"].as_str().unwrap().contains("cannot be reached"), "{list}");
    assert_eq!(list["packs"][0]["code"], "xx");
    let (status, body) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "xx" }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");

    // A catalogue of an unknown format is not read.
    let catalogue = start_catalogue().await;
    catalogue.files.lock().unwrap().insert("/languages/index.json".into(), Served::Body(br#"{"format": 2, "packs": []}"#.to_vec()));
    let (app, _, _, admin) = with_catalogue(Some(&catalogue.base)).await;
    let (_, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert!(list["catalogue_error"].as_str().unwrap().contains("format 2"), "{list}");

    // Off: nothing asked, nothing to install.
    let (app, _, _, admin) = with_catalogue(None).await;
    let (status, list) = send(&app, Method::GET, "/api/admin/languages", Some(&admin), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["catalogue_url"], Value::Null);
    assert_eq!(list["catalogue_error"], Value::Null);
    let (status, _) = send(&app, Method::POST, "/api/admin/languages/install", Some(&admin), Some(serde_json::json!({ "code": "xx" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

/// A book counted with a study list, as the built-in packs of old gave one.
async fn counted_with_a_study_list(db: &sqlx::AnyPool, owner: i64, language: &str) {
    book_in(db, owner, &format!("Book in {language}"), language).await;
    sqlx::query(
        "INSERT INTO vocabulary (file_sha256, language, version, words, stems, hapax, sample_need, coverage, curve, candidates, study, pack)
         VALUES ($1, $2, 1, 1, 1, 1, 0, '{}', '[]', '[]', '[]', $3)",
    )
    .bind(format!("sha-{language}"))
    .bind(language)
    .bind(format!("{language}:old:0"))
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn an_instance_that_used_the_built_in_packs_gets_them_from_the_catalogue_once() {
    let catalogue = start_catalogue().await;
    let es = pack_zip("es", "to run", "2026-11-08");
    let de = pack_zip("de", "to run", "2026-11-08");
    let index = serde_json::json!({ "format": 1, "packs": [offer("es", "Español", "2026-11-08", &es, None), offer("de", "Deutsch", "2026-11-08", &de, None)] });
    {
        let mut files = catalogue.files.lock().unwrap();
        files.insert("/languages/index.json".into(), Served::Body(index.to_string().into_bytes()));
        files.insert("/languages/es.zip".into(), Served::Body(es.clone()));
    }
    let (app, db, state, admin) = with_catalogue(Some(&catalogue.base)).await;
    let (bob, _) = add_user(&db, "bob").await;
    // Spanish and German were in use, French books were never counted with a list.
    counted_with_a_study_list(&db, bob, "es").await;
    counted_with_a_study_list(&db, bob, "de").await;
    book_in(&db, bob, "Livre", "fr").await;

    // German's zip is missing: Spanish comes, German is tried again later.
    let err = crate::langcatalogue::upgrade_once(&state).await.err().map(|e| format!("{e:#}")).unwrap_or_default();
    assert!(err.contains("de:"), "{err}");
    assert!(state.packs.get("es").is_some());
    assert!(state.packs.get("de").is_none() && state.packs.get("fr").is_none());
    let setting = |key: &'static str| {
        let db = db.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = $1").bind(key).fetch_optional(&db).await.unwrap() }
    };
    assert_eq!(setting("language_packs_pending").await.as_deref(), Some("de"));
    assert_eq!(setting("language_packs_migrated").await, None);

    // Meanwhile the books were counted again without a pack: German is still due.
    sqlx::query("UPDATE vocabulary SET study = NULL").execute(&db).await.unwrap();
    catalogue.files.lock().unwrap().insert("/languages/de.zip".into(), Served::Body(de.clone()));
    crate::langcatalogue::upgrade_once(&state).await.unwrap();
    assert!(state.packs.get("de").is_some());
    assert_eq!(setting("language_packs_migrated").await.as_deref(), Some("true"));
    let logged: Vec<String> = sqlx::query_scalar("SELECT details FROM activity_log WHERE action = 'admin.language_installed' AND actor_id IS NULL ORDER BY id")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(logged.len(), 2, "{logged:?}");

    // An admin removes Spanish: it stays removed.
    let (status, _) = send(&app, Method::DELETE, "/api/admin/languages/es", Some(&admin), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut upgrade = crate::langcatalogue::Upgrade::default();
    crate::langcatalogue::upgrade_built_in(&state, &mut upgrade).await;
    assert!(state.packs.get("es").is_none());
    assert!(!state.packs.dir().join("es").exists());

    // A second process that installs a pack there already leaves it as it is.
    let (pack, now) = match crate::langadmin::install_zip(&state, de.clone(), Some("de"), true).await {
        Ok(done) => done,
        Err(_) => panic!("install_zip failed"),
    };
    assert!(!now);
    assert_eq!(pack.meta.language, "de");
    assert!(hidden_dirs(&state).is_empty(), "{:?}", hidden_dirs(&state));
    // And a pack for another language than asked for is refused.
    assert!(crate::langadmin::install_zip(&state, es.clone(), Some("de"), false).await.is_err());
}

#[tokio::test]
async fn a_new_instance_gets_no_pack_by_itself() {
    let catalogue = start_catalogue().await;
    let es = pack_zip("es", "to run", "1");
    let index = serde_json::json!({ "format": 1, "packs": [offer("es", "Español", "1", &es, None)] });
    catalogue.files.lock().unwrap().insert("/languages/index.json".into(), Served::Body(index.to_string().into_bytes()));
    catalogue.files.lock().unwrap().insert("/languages/es.zip".into(), Served::Body(es));
    let (_, db, state, _) = with_catalogue(Some(&catalogue.base)).await;
    let (bob, _) = add_user(&db, "bob").await;
    book_in(&db, bob, "Libro", "es").await;
    crate::langcatalogue::upgrade_once(&state).await.unwrap();
    assert!(state.packs.get("es").is_none());
    let done: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'language_packs_migrated'").fetch_optional(&db).await.unwrap();
    assert_eq!(done.as_deref(), Some("true"));
    assert!(catalogue.seen.lock().unwrap().is_empty(), "nothing asked");
}
