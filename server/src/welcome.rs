//! The book a new library begins with: The Legejo Cookbook, the user manual,
//! put in the first admin's library when the instance is first started. It
//! is built from cookbook/ in the repository and carried in the binary.
//! After that it is a book like any other, and can be deleted.

use crate::license::License;
use crate::AppState;

/// The manual as an EPUB (cookbook/build.py writes it).
pub const COOKBOOK: &[u8] = include_bytes!("../assets/the-legejo-cookbook.epub");
const FILE_NAME: &str = "the-legejo-cookbook.epub";
/// The same manual in Esperanto.
pub const COOKBOOK_EO: &[u8] = include_bytes!("../assets/the-legejo-cookbook.eo.epub");

/// The manual in a language, when there is an edition in it: the file, its
/// name and the title it carries.
pub fn edition(language: &str) -> Option<(&'static [u8], &'static str, &'static str)> {
    match language {
        "en" => Some((COOKBOOK, FILE_NAME, "The Legejo Cookbook")),
        "eo" => Some((COOKBOOK_EO, "the-legejo-cookbook.eo.epub", "La Kuirlibro de Legejo")),
        _ => None,
    }
}
/// Where the book comes from: its text is in the public domain (CC0).
const SOURCE: &str = "https://github.com/jularboolean/legejo";

/// Put the manual in a user's library, with its licence stated. A failure is
/// logged and nothing more: a library without its manual still works.
pub async fn shelve(state: &AppState, owner_id: i64) -> Option<i64> {
    shelve_edition(state, owner_id, COOKBOOK, FILE_NAME).await
}

async fn shelve_edition(state: &AppState, owner_id: i64, file: &[u8], file_name: &str) -> Option<i64> {
    let book = match crate::books::store_book(state, owner_id, file, file_name).await {
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

#[derive(serde::Deserialize)]
pub struct AddManual {
    language: String,
}

/// POST /api/account/manual: the manual in a language put in the caller's
/// library, unless it is there already. Instances started before an
/// edition existed get it this way, and so does anyone reading Legejo in
/// another language than the first admin.
pub async fn add_manual(
    axum::extract::State(state): axum::extract::State<AppState>,
    user: crate::auth::AuthUser,
    axum::Json(req): axum::Json<AddManual>,
) -> Result<axum::Json<serde_json::Value>, axum::response::Response> {
    use axum::response::IntoResponse;
    let Some((file, file_name, title)) = edition(&req.language) else {
        return Err((axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({ "error": "no manual in that language" }))).into_response());
    };
    let there: Option<i64> = sqlx::query_scalar("SELECT id FROM books WHERE owner_id = $1 AND title = $2 ORDER BY id LIMIT 1")
        .bind(user.0.id)
        .bind(title)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("internal error: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })?;
    if let Some(id) = there {
        return Ok(axum::Json(serde_json::json!({ "book_id": id, "added": false })));
    }
    match shelve_edition(&state, user.0.id, file, file_name).await {
        Some(id) => Ok(axum::Json(serde_json::json!({ "book_id": id, "added": true }))),
        None => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    }
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

    #[tokio::test]
    async fn the_manual_in_esperanto_is_added_once_on_request() {
        let (app, db, _) = test_app().await;
        let (_, cookie) = add_user(&db, "zamenhof").await;
        let (status, v) = send(&app, Method::POST, "/api/account/manual", Some(&cookie), Some(serde_json::json!({ "language": "eo" }))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["added"], true);
        let id = v["book_id"].as_i64().unwrap();
        let (_, book) = send(&app, Method::GET, &format!("/api/books/{id}"), Some(&cookie), None).await;
        assert_eq!(book["title"], "La Kuirlibro de Legejo");
        assert_eq!(book["author"], "La Kolomboj de Legejo");
        assert_eq!(book["language"], "eo");
        assert_eq!(book["license"], "cc0");
        let issues = book["health"]["issues"].as_array().map(Vec::len).unwrap_or(0);
        assert_eq!(issues, 0, "{}", book["health"]);
        // Asked again: the same book, not a second one.
        let (_, again) = send(&app, Method::POST, "/api/account/manual", Some(&cookie), Some(serde_json::json!({ "language": "eo" }))).await;
        assert_eq!((again["book_id"].as_i64(), again["added"].as_bool()), (Some(id), Some(false)));
        let (status, _) = send(&app, Method::POST, "/api/account/manual", Some(&cookie), Some(serde_json::json!({ "language": "tlh" }))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn the_manual_in_esperanto_covers_the_same_version() {
        let covers = format!("Priskribas Legejo {}.{}.", env!("CARGO_PKG_VERSION_MAJOR"), env!("CARGO_PKG_VERSION_MINOR"));
        let sections = crate::booktext::sections(COOKBOOK_EO.to_vec()).expect("the Esperanto manual is a readable EPUB");
        assert!(sections.iter().any(|s| s.text.contains(&covers)), "the manual does not say \"{covers}\"; run python3 cookbook/build.py");
        assert!(sections.len() >= 14);
        assert!(!crate::epubfix::claims_copyright(COOKBOOK_EO));
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
