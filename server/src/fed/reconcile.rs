//! Keeps what followers have been told in step with what the shelves hold.
//!
//! Books reach and leave shelves by several paths (the web, Kobo
//! collections, deleting a book, a shelf changing visibility). Instead of a
//! hook in each, the reconciler compares the federated shelves with
//! ap_published and sends Create, Update or Delete for the difference.
//! Every book is gated again on the way out; one that fails while on a
//! federated shelf means the database is inconsistent and is logged as an
//! error.

use super::objects::{self, FedShelf, OutBook, FED_SHELF_SELECT};
use crate::books::{Book, BOOK_COLUMNS_B};
use crate::db::now_ts;
use crate::license;
use crate::AppState;
use serde_json::json;

/// A book on a federated shelf, as the outside sees it, with its hash.
pub struct Published {
    pub book: Book,
    pub sha256: String,
    pub cover_mime: Option<String>,
}

/// The federable books on a shelf, newest first. Books failing the gate are
/// left out and logged.
pub async fn shelf_books(state: &AppState, shelf_id: i64) -> anyhow::Result<Vec<Published>> {
    let rows: Vec<Book> = sqlx::query_as(&format!(
        "SELECT {BOOK_COLUMNS_B} FROM books b JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1 AND b.file_sha256 IS NOT NULL
         ORDER BY sb.added_at DESC, b.id DESC"
    ))
    .bind(shelf_id)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for book in rows {
        if let Err(reason) = license::federable(&book.license_facts(), license::current_year()) {
            tracing::error!(
                "fed: inconsistent database: book {} ({}) is on federated shelf {shelf_id} but fails the license gate: {}",
                book.id,
                book.uuid,
                serde_json::to_string(&reason).unwrap_or_default()
            );
            continue;
        }
        let (sha256, cover_mime): (String, Option<String>) =
            sqlx::query_as("SELECT file_sha256, cover_mime FROM books WHERE id = $1")
                .bind(book.id)
                .fetch_one(&state.db)
                .await?;
        out.push(Published { book, sha256, cover_mime });
    }
    Ok(out)
}

pub async fn federated_shelves(state: &AppState) -> anyhow::Result<Vec<FedShelf>> {
    Ok(sqlx::query_as(&format!(
        "{FED_SHELF_SELECT} WHERE s.visibility = 'federated' AND s.ap_slug IS NOT NULL"
    ))
    .fetch_all(&state.db)
    .await?)
}

pub async fn run(state: &AppState) -> anyhow::Result<()> {
    let Some(c) = super::active(state).await else { return Ok(()) };
    let shelves = federated_shelves(state).await?;
    for shelf in &shelves {
        let actor = c.shelf_actor(&shelf.ap_slug);
        let current = shelf_books(state, shelf.id).await?;
        let published: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT book_id, book_uuid, object_hash FROM ap_published WHERE shelf_id = $1")
                .bind(shelf.id)
                .fetch_all(&state.db)
                .await?;
        let inboxes = super::deliver::follower_inboxes(state, shelf.id).await?;

        for p in &current {
            let object = objects::book_object(&c, &shelf.ap_slug, &OutBook { book: &p.book, sha256: &p.sha256, cover_mime: p.cover_mime.as_deref() });
            let hash = crate::books::sha256_hex(object.to_string().as_bytes());
            let previous = published.iter().find(|(id, _, _)| *id == p.book.id);
            let kind = match previous {
                None => "Create",
                Some((_, _, h)) if *h != hash => "Update",
                Some(_) => continue,
            };
            let mut object = object;
            object["published"] = json!(now_ts());
            if !inboxes.is_empty() {
                let activity = objects::activity(&c, kind, &actor, object);
                super::deliver::enqueue(state, &inboxes, &actor, &activity).await?;
            }
            sqlx::query(
                "INSERT INTO ap_published (shelf_id, book_id, book_uuid, object_hash, published_at)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (shelf_id, book_id) DO UPDATE SET object_hash = excluded.object_hash",
            )
            .bind(shelf.id)
            .bind(p.book.id)
            .bind(&p.book.uuid)
            .bind(&hash)
            .bind(now_ts())
            .execute(&state.db)
            .await?;
            tracing::info!("fed: {kind} {} on {}", p.book.uuid, shelf.ap_slug);
        }

        for (book_id, uuid, _) in &published {
            if current.iter().any(|p| p.book.id == *book_id) {
                continue;
            }
            if !inboxes.is_empty() {
                let tombstone = json!({ "type": "Tombstone", "id": c.book_iri(uuid) });
                let activity = objects::activity(&c, "Delete", &actor, tombstone);
                super::deliver::enqueue(state, &inboxes, &actor, &activity).await?;
            }
            sqlx::query("DELETE FROM ap_published WHERE shelf_id = $1 AND book_id = $2")
                .bind(shelf.id)
                .bind(book_id)
                .execute(&state.db)
                .await?;
            tracing::info!("fed: Delete {uuid} from {}", shelf.ap_slug);
        }
    }

    // Shelves that stopped federating (but still exist): retract the actor.
    let stopped: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.ap_slug FROM shelves s
         WHERE s.visibility <> 'federated' AND s.ap_slug IS NOT NULL
           AND (EXISTS (SELECT 1 FROM ap_published p WHERE p.shelf_id = s.id)
                OR EXISTS (SELECT 1 FROM ap_followers f WHERE f.shelf_id = s.id))",
    )
    .fetch_all(&state.db)
    .await?;
    for (shelf_id, slug) in stopped {
        if let Some(slug) = slug {
            retract_shelf(state, &c, shelf_id, &slug).await?;
        }
    }
    // Shelves that were deleted outright.
    sqlx::query("DELETE FROM ap_published WHERE shelf_id NOT IN (SELECT id FROM shelves)")
        .execute(&state.db)
        .await?;
    Ok(())
}

/// Send Delete{Service} to the followers, then forget them.
pub async fn retract_shelf(state: &AppState, c: &super::FedConfig, shelf_id: i64, slug: &str) -> anyhow::Result<()> {
    let actor = c.shelf_actor(slug);
    let inboxes = super::deliver::follower_inboxes(state, shelf_id).await?;
    if !inboxes.is_empty() {
        let activity = json!({
            "@context": objects::context(),
            "id": super::activity_id(c),
            "type": "Delete",
            "actor": actor,
            "object": actor,
            "to": [super::AS_PUBLIC],
        });
        super::deliver::enqueue(state, &inboxes, &actor, &activity).await?;
    }
    sqlx::query("DELETE FROM ap_followers WHERE shelf_id = $1").bind(shelf_id).execute(&state.db).await?;
    sqlx::query("DELETE FROM ap_published WHERE shelf_id = $1").bind(shelf_id).execute(&state.db).await?;
    tracing::info!("fed: shelf {slug} no longer federated; Delete sent to {} inboxes", inboxes.len());
    Ok(())
}
