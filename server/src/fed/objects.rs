//! JSON-LD documents: actors, the book object, activities, WebFinger and
//! nodeinfo.

use super::FedConfig;
use crate::books::Book;
use serde_json::{json, Value};

/// The extra terms Legejo reads; Mastodon ignores them and shows
/// `name`/`summary`/`content`.
pub fn context() -> Value {
    json!([
        "https://www.w3.org/ns/activitystreams",
        "https://w3id.org/security/v1",
        {
            "legejo": "https://github.com/jularboolean/legejo/ns#",
            "license": "legejo:license",
            "licenseSource": "legejo:licenseSource",
            "authorDeathYear": "legejo:authorDeathYear",
            "isbn": "legejo:isbn",
            "sha256": "legejo:sha256",
            "size": "legejo:size"
        }
    ])
}

fn public_key(actor: &str, pem: &str) -> Value {
    json!({ "id": format!("{actor}#main-key"), "owner": actor, "publicKeyPem": pem })
}

/// The instance actor: signs fetches, sends Follows on behalf of users.
pub fn instance_actor(c: &FedConfig, pem: &str) -> Value {
    let id = c.instance_actor();
    json!({
        "@context": context(),
        "type": "Application",
        "id": id,
        "preferredUsername": c.host,
        "name": "Legejo",
        "summary": "Legejo instance actor",
        "inbox": c.shared_inbox(),
        "outbox": format!("{id}/outbox"),
        "url": c.base,
        "manuallyApprovesFollowers": true,
        "endpoints": { "sharedInbox": c.shared_inbox() },
        "publicKey": public_key(&id, pem),
    })
}

/// What an actor document needs to know about a shelf.
#[derive(sqlx::FromRow, Clone, Debug)]
pub struct FedShelf {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub ap_slug: String,
    pub owner: String,
    pub has_cover: crate::db::DbFlag,
    pub created_at: String,
}

pub const FED_SHELF_SELECT: &str = "SELECT s.id, s.name, s.description, s.ap_slug, u.username AS owner,
        CAST(CASE WHEN s.cover_mime IS NOT NULL THEN 1 ELSE 0 END AS BIGINT) AS has_cover, s.created_at
     FROM shelves s JOIN users u ON u.id = s.owner_id";

pub fn shelf_actor(c: &FedConfig, shelf: &FedShelf, pem: &str) -> Value {
    let id = c.shelf_actor(&shelf.ap_slug);
    let mut actor = json!({
        "@context": context(),
        "type": "Service",
        "id": id,
        "preferredUsername": shelf.ap_slug,
        "name": shelf.name,
        "summary": shelf.description.as_deref().map(html_paragraphs).unwrap_or_default(),
        "url": format!("{}/f/{}", c.base, shelf.ap_slug),
        "inbox": format!("{id}/inbox"),
        "outbox": format!("{id}/outbox"),
        "followers": format!("{id}/followers"),
        "manuallyApprovesFollowers": false,
        "discoverable": true,
        "published": shelf.created_at,
        "attachment": [{ "type": "PropertyValue", "name": "Legejo", "value": format!("{} · {}", shelf.owner, c.host) }],
        "endpoints": { "sharedInbox": c.shared_inbox() },
        "publicKey": public_key(&id, pem),
    });
    if shelf.has_cover.as_bool() {
        actor["icon"] = json!({ "type": "Image", "url": format!("{id}/icon") });
    }
    actor
}

/// The facts of a book as it goes out.
pub struct OutBook<'a> {
    pub book: &'a Book,
    pub sha256: &'a str,
}

/// The Page object for a book on a shelf. `published` is left out here and
/// added by the caller, so the hash of this value only changes with content.
pub fn book_object(c: &FedConfig, slug: &str, b: &OutBook) -> Value {
    let book = b.book;
    let id = c.book_iri(&book.uuid);
    let shelf = c.shelf_actor(slug);
    let license = book.license.as_deref().unwrap_or("");
    let by = book.author.as_deref().map(|a| format!(" by {a}")).unwrap_or_default();
    let mut summary = format!("{}{by}. License: {license}.", book.title);
    if let Some(year) = book.author_death_year {
        summary.push_str(&format!(" The author died in {year}."));
    }
    let page_url = format!("{}/f/{slug}/{}", c.base, book.uuid);
    json!({
        "@context": context(),
        "type": "Page",
        "id": id,
        "attributedTo": shelf,
        "name": book.title,
        "summary": summary,
        "content": format!(
            "<p><a href=\"{page_url}\">{}</a>{}</p>{}",
            escape(&book.title),
            escape(&by),
            book.description.as_deref().map(html_paragraphs).unwrap_or_default()
        ),
        "url": [
            { "type": "Link", "mediaType": "text/html", "href": page_url },
            { "type": "Link", "mediaType": "application/epub+zip", "href": format!("{id}/epub"),
              "sha256": b.sha256, "size": book.file_size }
        ],
        "icon": { "type": "Image", "url": format!("{id}/cover") },
        "attributedToName": book.author,
        "author": book.author,
        "language": book.language,
        "license": license,
        "licenseSource": book.license_source_url,
        "authorDeathYear": book.author_death_year,
        "isbn": book.isbn,
        "to": [super::AS_PUBLIC],
        "cc": [format!("{shelf}/followers")],
    })
}

/// Wrap an object in an activity from `actor`.
pub fn activity(c: &FedConfig, kind: &str, actor: &str, object: Value) -> Value {
    let mut object = object;
    if let Some(o) = object.as_object_mut() {
        o.remove("@context");
    }
    json!({
        "@context": context(),
        "id": super::activity_id(c),
        "type": kind,
        "actor": actor,
        "object": object,
        "to": [super::AS_PUBLIC],
        "cc": [format!("{actor}/followers")],
    })
}

pub fn webfinger(c: &FedConfig, slug: &str) -> Value {
    let actor = c.shelf_actor(slug);
    json!({
        "subject": format!("acct:{slug}@{}", c.host),
        "aliases": [actor, format!("{}/f/{slug}", c.base)],
        "links": [
            { "rel": "self", "type": super::AP_JSON, "href": actor },
            { "rel": "http://webfinger.net/rel/profile-page", "type": "text/html", "href": format!("{}/f/{slug}", c.base) }
        ]
    })
}

pub fn nodeinfo(mode: &str, contact: Option<&str>, shelves: i64, books: i64) -> Value {
    json!({
        "version": "2.1",
        "software": {
            "name": "legejo",
            "version": env!("CARGO_PKG_VERSION"),
            "repository": "https://github.com/jularboolean/legejo",
        },
        "protocols": ["activitypub"],
        "services": { "inbound": [], "outbound": [] },
        "openRegistrations": false,
        "usage": { "users": {}, "localPosts": books },
        "metadata": {
            "federationMode": mode,
            "contact": contact,
            "federatedShelves": shelves,
        }
    })
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Plain text (Markdown in the app) as safe HTML paragraphs.
pub fn html_paragraphs(s: &str) -> String {
    s.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", escape(p).replace('\n', "<br>")))
        .collect()
}
