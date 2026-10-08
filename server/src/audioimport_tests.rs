//! The import folder for audiobooks.

use crate::settings::ImportDir;
use crate::testutil::{add_user, send, test_app};
use axum::http::Method;
use serde_json::json;
use std::time::Duration;

async fn titles(db: &sqlx::AnyPool, owner: i64) -> Vec<String> {
    sqlx::query_scalar("SELECT title FROM audiobooks WHERE owner_id = $1 ORDER BY title").bind(owner).fetch_all(db).await.unwrap()
}

#[tokio::test]
async fn the_audiobook_folder_makes_an_audiobook_of_each_entry() {
    let dir = std::env::temp_dir().join(format!("legejo-audio-import-{}", crate::books::new_uuid()));
    let book = dir.join("Röda rummet");
    std::fs::create_dir_all(book.join("CD 10")).unwrap();
    std::fs::create_dir_all(book.join("CD 2")).unwrap();
    std::fs::write(book.join("CD 10/1.mp3"), b"third").unwrap();
    std::fs::write(book.join("CD 2/10.mp3"), b"second!").unwrap();
    std::fs::write(book.join("CD 2/9.mp3"), b"first!!!").unwrap();
    std::fs::write(book.join("cover.jpg"), b"the picture").unwrap();
    std::fs::write(book.join("booklet.pdf"), b"left alone").unwrap();
    std::fs::write(dir.join("Doktor_Glas_64kb.m4b"), b"one file").unwrap();
    std::fs::create_dir_all(dir.join("Papers")).unwrap();
    std::fs::write(dir.join("Papers/notes.txt"), b"no audio here").unwrap();

    let config = ImportDir { dir: dir.clone(), user: Some("carol".into()), interval_secs: 60, delete: false };
    let (app, db, state) = test_app().await;
    let (admin, admin_cookie) = add_user(&db, "admin").await;
    sqlx::query("UPDATE users SET is_admin = 1 WHERE id = ?").bind(admin).execute(&db).await.unwrap();
    let (carol, carol_cookie) = add_user(&db, "carol").await;

    // Nothing is taken while audiobooks are turned off, and nothing is moved.
    assert!(crate::audioimport::scan(&state, &config, Duration::ZERO).await.is_err());
    assert_eq!(crate::audioimport::pending(&dir), 2);
    let (_, settings) = send(&app, Method::GET, "/api/admin/settings", Some(&admin_cookie), None).await;
    let mut settings = settings;
    settings["audiobooks_enabled"] = json!(true);
    send(&app, Method::PUT, "/api/admin/settings", Some(&admin_cookie), Some(settings)).await;

    // Fresh entries wait until they have settled.
    assert_eq!(crate::audioimport::scan(&state, &config, Duration::from_secs(3600)).await.unwrap(), 0);
    assert_eq!(crate::audioimport::scan(&state, &config, Duration::ZERO).await.unwrap(), 2);
    assert_eq!(titles(&db, carol).await, ["Doktor Glas", "Röda rummet"]);
    assert_eq!(crate::audioimport::pending(&dir), 0);

    // The parts are heard in the order of their folders and names, numbers by value.
    let parts: Vec<(String, i64)> = sqlx::query_as(
        "SELECT f.filename, f.bytes FROM audiobook_files f JOIN audiobooks a ON a.id = f.audiobook_id WHERE a.title = 'Röda rummet' ORDER BY f.position",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert_eq!(parts, [("9.mp3".to_string(), 8), ("10.mp3".to_string(), 7), ("1.mp3".to_string(), 5)]);
    let single: String =
        sqlx::query_scalar("SELECT f.filename FROM audiobook_files f JOIN audiobooks a ON a.id = f.audiobook_id WHERE a.title = 'Doktor Glas'").fetch_one(&db).await.unwrap();
    assert_eq!(single, "Doktor_Glas_64kb.m4b");
    let (uuid, cover): (String, Option<String>) =
        sqlx::query_as("SELECT uuid, cover_mime FROM audiobooks WHERE title = 'Röda rummet'").fetch_one(&db).await.unwrap();
    assert_eq!(cover.as_deref(), Some("image/jpeg"));
    assert_eq!(std::fs::read(state.data_dir.join("audio_covers").join(&uuid)).unwrap(), b"the picture");
    let stored: Vec<_> = std::fs::read_dir(state.data_dir.join("audio").join(&uuid)).unwrap().flatten().collect();
    assert_eq!(stored.len(), 3);

    // The owner sees them as any other audiobook of theirs.
    let (_, list) = send(&app, Method::GET, "/api/audiobooks", Some(&carol_cookie), None).await;
    assert_eq!(list.as_array().map(Vec::len), Some(2), "{list}");

    // The originals are kept, whole, beside what had no audio.
    assert!(dir.join("imported/Röda rummet/CD 2/9.mp3").exists());
    assert!(dir.join("imported/Röda rummet/booklet.pdf").exists());
    assert!(dir.join("imported/Doktor_Glas_64kb.m4b").exists());
    assert!(dir.join("Papers/notes.txt").exists());

    // The same files again are a duplicate, not a second audiobook.
    std::fs::create_dir_all(dir.join("Again/CD 2")).unwrap();
    std::fs::write(dir.join("Again/CD 2/9.mp3"), b"first!!!").unwrap();
    std::fs::write(dir.join("Again/CD 2/10.mp3"), b"second!").unwrap();
    std::fs::write(dir.join("Again/CD 10 1.mp3"), b"third").unwrap();
    assert_eq!(crate::audioimport::scan(&state, &config, Duration::ZERO).await.unwrap(), 0);
    assert!(dir.join("duplicates/Again").is_dir());
    assert_eq!(titles(&db, carol).await.len(), 2);
    let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'audiobook.folder_imported'").fetch_one(&db).await.unwrap();
    assert_eq!(logged, 2);

    // Without a named user the oldest admin gets them; delete mode removes the entry.
    let config = ImportDir { user: None, delete: true, ..config };
    std::fs::create_dir_all(dir.join("Hemsöborna")).unwrap();
    std::fs::write(dir.join("Hemsöborna/01.mp3"), b"a").unwrap();
    std::fs::write(dir.join("Hemsöborna/02.mp3"), b"bb").unwrap();
    assert_eq!(crate::audioimport::scan(&state, &config, Duration::ZERO).await.unwrap(), 1);
    assert_eq!(titles(&db, admin).await, ["Hemsöborna"]);
    assert!(!dir.join("Hemsöborna").exists() && !dir.join("imported/Hemsöborna").exists());

    // A user that does not exist stops the scan before anything is claimed.
    let config = ImportDir { user: Some("nobody".into()), ..config };
    std::fs::write(dir.join("later.mp3"), b"zzzz").unwrap();
    assert!(crate::audioimport::scan(&state, &config, Duration::ZERO).await.is_err());
    assert!(dir.join("later.mp3").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
