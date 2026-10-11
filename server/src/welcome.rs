//! The book a new library begins with: The Legejo Cookbook, the user manual,
//! put in the first admin's library when the instance is first started. It
//! is built from cookbook/ in the repository and carried in the binary.
//! After that it is a book like any other, and can be deleted.

use crate::license::License;
use crate::AppState;

/// The manual as an EPUB (cookbook/build.py writes it).
pub const COOKBOOK: &[u8] = include_bytes!("../assets/the-legejo-cookbook.epub");
const FILE_NAME: &str = "the-legejo-cookbook.epub";
/// Where the book comes from: its text is in the public domain (CC0).
const SOURCE: &str = "https://github.com/jularboolean/legejo";

/// Put the manual in a user's library, with its licence stated. A failure is
/// logged and nothing more: a library without its manual still works.
pub async fn shelve(state: &AppState, owner_id: i64) -> Option<i64> {
    let book = match crate::books::store_book(state, owner_id, COOKBOOK, FILE_NAME).await {
        Ok(Ok(book)) => book,
        Ok(Err(e)) => {
            tracing::warn!("the manual could not be added to the new library: {e}");
            return None;
        }
        Err(_) => {
            tracing::warn!("the manual could not be added to the new library");
            return None;
        }
    };
    let stated = sqlx::query("UPDATE books SET license = $1, license_source_url = $2, cover_is_free = 1 WHERE id = $3")
        .bind(License::Cc0.as_str())
        .bind(SOURCE)
        .bind(book.id)
        .execute(&state.db)
        .await;
    if let Err(e) = stated {
        tracing::warn!("the manual's licence could not be recorded: {e}");
    }
    crate::audit::log(state, None, "book.welcome", serde_json::json!({ "book_id": book.id, "title": book.title })).await;
    tracing::info!("a new library: \"{}\" is on its shelf", book.title);
    Some(book.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{add_user, send, test_app};
    use axum::http::{Method, StatusCode};

    #[tokio::test]
    async fn a_new_library_begins_with_its_manual() {
        let (app, db, state) = test_app().await;
        let (alice, cookie) = add_user(&db, "alice").await;
        let id = shelve(&state, alice).await.expect("the manual is added");

        let (status, book) = send(&app, Method::GET, &format!("/api/books/{id}"), Some(&cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{book}");
        assert_eq!(book["title"], "The Legejo Cookbook");
        assert_eq!(book["author"], "The Pigeons of Legejo");
        assert_eq!(book["language"], "en");
        assert_eq!(book["format"], "epub");
        assert_eq!(book["has_cover"], true);
        assert_eq!(book["license"], "cc0", "free to share, by what the book itself says and what is recorded");
        assert_eq!(book["license_source_url"], SOURCE);
        // Nothing for the file check to remark on, or to mend.
        let issues = book["health"]["issues"].as_array().map(Vec::len).unwrap_or(0);
        assert_eq!(issues, 0, "{}", book["health"]);
        assert!(!crate::epubfix::claims_copyright(COOKBOOK));

        // It is the owner's to delete.
        let (status, _) = send(&app, Method::DELETE, &format!("/api/books/{id}"), Some(&cookie), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[test]
    fn the_manual_covers_the_version_it_ships_with() {
        // cookbook/build.py writes the version into the book. Run it again
        // when this fails: the text is due a look for every minor version.
        let covers = format!("Covers Legejo {}.{}.", env!("CARGO_PKG_VERSION_MAJOR"), env!("CARGO_PKG_VERSION_MINOR"));
        let sections = crate::booktext::sections(COOKBOOK.to_vec()).expect("the manual is a readable EPUB");
        assert!(sections.iter().any(|s| s.text.contains(&covers)), "the manual does not say \"{covers}\"; run python3 cookbook/build.py");
        assert!(sections.len() >= 14, "preface, twelve chapters and a colophon at least: {}", sections.len());
    }
}
