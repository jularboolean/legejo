//! Following a shared shelf on this instance: a user who can see a shelf
//! another user shares may follow it, and is then told what has come onto
//! it since they last looked. "Since they last looked" is `seen_at`: it is
//! set when the shelf's page is opened (public::shelf) and when the user
//! says they have seen it all.
//!
//! Each follow has an Atom feed of its own, under a key that is the only
//! credential (like the podcast feeds), so a feed reader can follow the shelf
//! without a login. The feed checks at every request that the user still
//! follows the shelf and may still see it; unfollowing ends the feed.
//!
//! Shelves followed from other instances (federation, private links) are
//! not covered here.

use crate::auth::AuthUser;
use crate::db::now_ts;
use crate::booktext::esc;
use crate::shelves::visible_to;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;

/// Books in a feed, newest on the shelf first.
const FEED_ENTRIES: i64 = 50;
/// Titles named per shelf in the summary of what is new.
const NEWS_TITLES: i64 = 3;

fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("internal error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
}

/// The web address of a shared shelf, or of a book on it, with the owner
/// and shelf name encoded as path segments.
pub(crate) fn shelf_url(base: &str, owner: &str, shelf: &str, book_uuid: Option<&str>) -> String {
    let Ok(mut url) = url::Url::parse(base) else {
        return format!("{base}/public");
    };
    if let Ok(mut path) = url.path_segments_mut() {
        path.pop_if_empty().extend(["public", owner, shelf]);
        if let Some(uuid) = book_uuid {
            path.push(uuid);
        }
    }
    url.to_string()
}

fn feed_url(base: &str, key: &str) -> String {
    format!("{base}/feeds/shelf/{key}")
}

/// The caller's follow of a shelf: (seen_at, feed_key).
pub(crate) async fn follow_of(state: &AppState, user_id: i64, shelf_id: i64) -> Result<Option<(String, String)>, sqlx::Error> {
    sqlx::query_as("SELECT seen_at, feed_key FROM shelf_follows WHERE user_id = $1 AND shelf_id = $2")
        .bind(user_id)
        .bind(shelf_id)
        .fetch_optional(&state.db)
        .await
}

/// Books put on the shelf after `since`.
pub(crate) async fn new_count(state: &AppState, shelf_id: i64, since: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM shelf_books WHERE shelf_id = $1 AND added_at > $2")
        .bind(shelf_id)
        .bind(since)
        .fetch_one(&state.db)
        .await
}

/// The caller has now seen the shelf.
pub(crate) async fn mark_seen(state: &AppState, user_id: i64, shelf_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE shelf_follows SET seen_at = $1 WHERE user_id = $2 AND shelf_id = $3")
        .bind(now_ts())
        .bind(user_id)
        .bind(shelf_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

/// What the shelf list and a shelf's page say about following it.
#[derive(Serialize, Default)]
pub struct FollowState {
    pub followed: bool,
    /// Books put on the shelf since the caller last looked.
    pub new_books: i64,
    /// When the caller last looked, so a page can mark what came after.
    pub seen_at: Option<String>,
    /// The caller's Atom feed of the shelf.
    pub feed: Option<String>,
}

pub(crate) async fn state_of(state: &AppState, headers: &HeaderMap, user_id: i64, shelf_id: i64) -> Result<FollowState, sqlx::Error> {
    let Some((seen_at, key)) = follow_of(state, user_id, shelf_id).await? else {
        return Ok(FollowState::default());
    };
    Ok(FollowState {
        followed: true,
        new_books: new_count(state, shelf_id, &seen_at).await?,
        feed: Some(feed_url(&crate::invite::base(state, headers), &key)),
        seen_at: Some(seen_at),
    })
}

/// PUT /api/public/shelves/{id}/follow: follow a shelf shared with the
/// caller. What is on it already counts as seen.
pub async fn follow(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(shelf_id): Path<i64>,
) -> Result<Json<FollowState>, Response> {
    let shelf: Option<(String, String)> = sqlx::query_as(&format!(
        "SELECT s.name, u.username FROM shelves s JOIN users u ON u.id = s.owner_id
         WHERE s.id = $1 AND s.owner_id <> $2 AND {visible}",
        visible = visible_to("$2"),
    ))
    .bind(shelf_id)
    .bind(user.0.id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    let (name, owner) = shelf.ok_or_else(not_found)?;
    if follow_of(&state, user.0.id, shelf_id).await.map_err(internal)?.is_none() {
        let now = now_ts();
        let inserted = sqlx::query(
            "INSERT INTO shelf_follows (user_id, shelf_id, followed_at, seen_at, feed_key) VALUES ($1, $2, $3, $3, $4)
             ON CONFLICT (user_id, shelf_id) DO NOTHING",
        )
        .bind(user.0.id)
        .bind(shelf_id)
        .bind(&now)
        .bind(crate::auth::new_token())
        .execute(&state.db)
        .await
        .map_err(internal)?;
        if inserted.rows_affected() > 0 {
            crate::audit::log(&state, crate::audit::by(&user.0), "shelf.followed", json!({ "shelf_id": shelf_id, "shelf": name, "owner": owner })).await;
        }
    }
    Ok(Json(state_of(&state, &headers, user.0.id, shelf_id).await.map_err(internal)?))
}

/// DELETE /api/public/shelves/{id}/follow: stop following; the feed stops too.
pub async fn unfollow(State(state): State<AppState>, user: AuthUser, Path(shelf_id): Path<i64>) -> Result<StatusCode, Response> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM shelves WHERE id = $1").bind(shelf_id).fetch_optional(&state.db).await.map_err(internal)?;
    let r = sqlx::query("DELETE FROM shelf_follows WHERE user_id = $1 AND shelf_id = $2")
        .bind(user.0.id)
        .bind(shelf_id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    if r.rows_affected() > 0 {
        crate::audit::log(&state, crate::audit::by(&user.0), "shelf.unfollowed", json!({ "shelf_id": shelf_id, "shelf": name })).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct NewTitle {
    id: i64,
    title: String,
    author: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ShelfNews {
    id: i64,
    name: String,
    owner: String,
    new_books: i64,
    /// When the latest of them was put on the shelf.
    latest: String,
    #[sqlx(skip)]
    titles: Vec<NewTitle>,
}

/// GET /api/follows/news: the followed shelves that have had books put on
/// them since the caller last looked, the most recent first. A shelf that
/// is no longer shared with the caller is left out.
pub async fn news(State(state): State<AppState>, user: AuthUser) -> Result<Json<Vec<ShelfNews>>, Response> {
    let mut shelves: Vec<ShelfNews> = sqlx::query_as(&format!(
        "SELECT s.id, s.name, u.username AS owner, COUNT(sb.book_id) AS new_books, MAX(sb.added_at) AS latest
         FROM shelf_follows f
         JOIN shelves s ON s.id = f.shelf_id
         JOIN users u ON u.id = s.owner_id
         JOIN shelf_books sb ON sb.shelf_id = s.id AND sb.added_at > f.seen_at
         WHERE f.user_id = $1 AND {visible}
         GROUP BY s.id, s.name, u.username
         ORDER BY MAX(sb.added_at) DESC",
        visible = visible_to("$1"),
    ))
    .bind(user.0.id)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    for shelf in &mut shelves {
        shelf.titles = sqlx::query_as(
            "SELECT b.id, b.title, b.author FROM books b
             JOIN shelf_books sb ON sb.book_id = b.id
             JOIN shelf_follows f ON f.shelf_id = sb.shelf_id AND f.user_id = $2
             WHERE sb.shelf_id = $1 AND sb.added_at > f.seen_at
             ORDER BY sb.added_at DESC, b.id DESC LIMIT $3",
        )
        .bind(shelf.id)
        .bind(user.0.id)
        .bind(NEWS_TITLES)
        .fetch_all(&state.db)
        .await
        .map_err(internal)?;
    }
    Ok(Json(shelves))
}

/// POST /api/follows/seen: everything on the followed shelves counts as seen.
pub async fn seen_all(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, Response> {
    sqlx::query("UPDATE shelf_follows SET seen_at = $1 WHERE user_id = $2")
        .bind(now_ts())
        .bind(user.0.id)
        .execute(&state.db)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ Atom

/// The shelf behind a feed key, if its follower may still see it:
/// (shelf id, name, description, owner, follower id).
async fn feed_shelf(state: &AppState, key: &str) -> Result<(i64, String, Option<String>, String), Response> {
    let row: Option<(i64, String, Option<String>, String)> = sqlx::query_as(&format!(
        "SELECT s.id, s.name, s.description, u.username FROM shelf_follows f
         JOIN shelves s ON s.id = f.shelf_id
         JOIN users u ON u.id = s.owner_id
         WHERE f.feed_key = $1 AND {visible}",
        visible = visible_to("f.user_id"),
    ))
    .bind(key)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    row.ok_or_else(|| (StatusCode::NOT_FOUND, "not found").into_response())
}

#[derive(sqlx::FromRow)]
struct FeedBook {
    id: i64,
    uuid: String,
    title: String,
    author: Option<String>,
    description: Option<String>,
    cover_mime: Option<String>,
    added_at: String,
}

/// GET /feeds/shelf/{key}: the shelf as an Atom feed, one entry per book,
/// dated when it was put on the shelf. Links lead to the shelf in the app,
/// where the follower is logged in.
pub async fn atom(State(state): State<AppState>, headers: HeaderMap, Path(key): Path<String>) -> Result<Response, Response> {
    let (shelf_id, name, description, owner) = feed_shelf(&state, &key).await?;
    let books: Vec<FeedBook> = sqlx::query_as(
        "SELECT b.id, b.uuid, b.title, b.author, b.description, b.cover_mime, sb.added_at FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = $1 ORDER BY sb.added_at DESC, b.id DESC LIMIT $2",
    )
    .bind(shelf_id)
    .bind(FEED_ENTRIES)
    .fetch_all(&state.db)
    .await
    .map_err(internal)?;
    let base = crate::invite::base(&state, &headers);
    let page = shelf_url(&base, &owner, &name, None);
    let feed = feed_url(&base, &key);
    let updated = books.first().map(|b| b.added_at.clone()).unwrap_or_else(now_ts);
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <feed xmlns=\"http://www.w3.org/2005/Atom\">\n\
         <id>{page}</id>\n<title>{title}</title>\n{subtitle}\
         <updated>{updated}</updated>\n<author><name>{owner}</name></author>\n\
         <link rel=\"self\" type=\"application/atom+xml\" href=\"{feed}\"/>\n\
         <link rel=\"alternate\" type=\"text/html\" href=\"{page}\"/>\n\
         <generator uri=\"https://github.com/jularboolean/legejo\">Legejo</generator>\n",
        page = esc(&page),
        title = esc(&name),
        subtitle = description.as_deref().filter(|d| !d.trim().is_empty()).map(|d| format!("<subtitle>{}</subtitle>\n", esc(d))).unwrap_or_default(),
        updated = esc(&updated),
        owner = esc(&owner),
        feed = esc(&feed),
    );
    for b in &books {
        let link = shelf_url(&base, &owner, &name, Some(&b.uuid));
        let mut summary = String::new();
        if b.cover_mime.is_some() {
            summary.push_str(&format!("<p><img src=\"{}\" alt=\"\" style=\"max-width:10em\"/></p>", esc(&format!("{feed}/books/{}/cover", b.id))));
        }
        if let Some(d) = b.description.as_deref().filter(|d| !d.trim().is_empty()) {
            // The description is HTML or text from the book; it goes in as
            // text, so nothing in it runs in the reader.
            summary.push_str(&format!("<p>{}</p>", esc(&crate::booktext::html_to_text(d))));
        }
        xml.push_str(&format!(
            "<entry>\n<id>{link}</id>\n<title>{title}</title>\n{author}<updated>{at}</updated>\n<published>{at}</published>\n\
             <link rel=\"alternate\" type=\"text/html\" href=\"{link}\"/>\n{summary}</entry>\n",
            link = esc(&link),
            title = esc(&b.title),
            author = b.author.as_deref().filter(|a| !a.trim().is_empty()).map(|a| format!("<author><name>{}</name></author>\n", esc(a))).unwrap_or_default(),
            at = esc(&b.added_at),
            summary = if summary.is_empty() { String::new() } else { format!("<summary type=\"html\">{}</summary>\n", esc(&summary)) },
        ));
    }
    xml.push_str("</feed>\n");
    Ok((
        [
            (header::CONTENT_TYPE, "application/atom+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "private, max-age=300"),
        ],
        xml,
    )
        .into_response())
}

/// GET /feeds/shelf/{key}/books/{id}/cover: a cover for the feed, for a
/// book that is on the shelf.
pub async fn cover(State(state): State<AppState>, Path((key, book_id)): Path<(String, i64)>) -> Result<Response, Response> {
    let (shelf_id, ..) = feed_shelf(&state, &key).await?;
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT b.uuid, b.cover_mime FROM books b JOIN shelf_books sb ON sb.book_id = b.id WHERE sb.shelf_id = $1 AND b.id = $2",
    )
    .bind(shelf_id)
    .bind(book_id)
    .fetch_optional(&state.db)
    .await
    .map_err(internal)?;
    let (uuid, mime) = row.ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    let mime = mime.ok_or_else(|| StatusCode::NOT_FOUND.into_response())?;
    let data = tokio::fs::read(state.data_dir.join("covers").join(&uuid)).await.map_err(|_| StatusCode::NOT_FOUND.into_response())?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "private, max-age=3600".to_string())], data).into_response())
}

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/shelf/{key}", axum::routing::get(atom))
        .route("/shelf/{key}/books/{id}/cover", axum::routing::get(cover))
        .fallback(|| async { StatusCode::NOT_FOUND })
}

#[cfg(test)]
mod tests {
    use crate::testutil::{add_book, add_user, send, test_app};
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use tower::ServiceExt;

    async fn get_text(app: &axum::Router, uri: &str) -> (StatusCode, String, String) {
        let res = app.clone().oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let kind = res.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
        let body = String::from_utf8(axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        (status, kind, body)
    }

    #[tokio::test]
    async fn follow_a_shared_shelf() {
        let (app, db, _) = test_app().await;
        let (alice, alice_cookie) = add_user(&db, "alice").await;
        let (_, bob_cookie) = add_user(&db, "bob").await;
        let shelf: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name, visibility, description) VALUES (?, 'Deckare & annat', 'instance', 'Spänning') RETURNING id")
            .bind(alice).fetch_one(&db).await.unwrap();
        let hidden: i64 = sqlx::query_scalar("INSERT INTO shelves (owner_id, name) VALUES (?, 'Privat') RETURNING id")
            .bind(alice).fetch_one(&db).await.unwrap();
        let (old, _) = add_book(&db, alice, "Roseanna").await;
        let (fresh, _) = add_book(&db, alice, "Mannen på balkongen").await;
        sqlx::query("UPDATE books SET author = 'Sjöwall & Wahlöö', description = '<p>Om <b>Beck</b></p>' WHERE id = ?").bind(fresh).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO shelf_books (shelf_id, book_id, added_at) VALUES (?, ?, '2010-01-01T00:00:00.000Z')").bind(shelf).bind(old).execute(&db).await.unwrap();

        // Only a shelf someone else shares with you can be followed.
        let (status, _) = send(&app, Method::PUT, &format!("/api/public/shelves/{shelf}/follow"), Some(&alice_cookie), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "not one's own");
        let (status, _) = send(&app, Method::PUT, &format!("/api/public/shelves/{hidden}/follow"), Some(&bob_cookie), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "not a private one");
        let (status, v) = send(&app, Method::PUT, &format!("/api/public/shelves/{shelf}/follow"), Some(&bob_cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["followed"].clone(), v["new_books"].clone()), (serde_json::json!(true), serde_json::json!(0)), "what is there is seen");
        let feed = v["feed"].as_str().unwrap().to_string();
        let path = &feed[feed.find("/feeds/").unwrap()..];
        let (status, again) = send(&app, Method::PUT, &format!("/api/public/shelves/{shelf}/follow"), Some(&bob_cookie), None).await;
        assert_eq!((status, again["feed"].as_str()), (StatusCode::OK, Some(feed.as_str())), "following twice keeps the feed");

        // A book put on the shelf after bob last looked is new to him.
        sqlx::query("UPDATE shelf_follows SET seen_at = '2020-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO shelf_books (shelf_id, book_id, added_at) VALUES (?, ?, '2025-05-05T00:00:00.000Z')").bind(shelf).bind(fresh).execute(&db).await.unwrap();
        let (_, news) = send(&app, Method::GET, "/api/follows/news", Some(&bob_cookie), None).await;
        let news = news.as_array().unwrap();
        assert_eq!(news.len(), 1);
        assert_eq!((news[0]["name"].as_str(), news[0]["owner"].as_str(), news[0]["new_books"].as_i64()), (Some("Deckare & annat"), Some("alice"), Some(1)));
        assert_eq!(news[0]["titles"][0]["title"], "Mannen på balkongen");
        let (_, list) = send(&app, Method::GET, "/api/public/shelves", Some(&bob_cookie), None).await;
        assert_eq!(list[0]["follow"]["new_books"], 1);
        let (_, alice_news) = send(&app, Method::GET, "/api/follows/news", Some(&alice_cookie), None).await;
        assert_eq!(alice_news.as_array().unwrap().len(), 0);

        // The feed: no login, newest first, links into the app, the
        // description as text and the cover through the feed.
        let (status, kind, xml) = get_text(&app, path).await;
        assert_eq!(status, StatusCode::OK);
        assert!(kind.starts_with("application/atom+xml"), "{kind}");
        assert!(xml.contains("<title>Deckare &amp; annat</title>"), "{xml}");
        assert!(xml.contains("<subtitle>Spänning</subtitle>"));
        assert!(xml.find("Mannen på balkongen").unwrap() < xml.find("Roseanna").unwrap(), "newest first");
        assert!(xml.contains("/public/alice/Deckare%20&amp;%20annat/"), "{xml}");
        assert!(xml.contains("<author><name>Sjöwall &amp; Wahlöö</name></author>"));
        assert!(xml.contains("Om Beck") && !xml.contains("&lt;b&gt;"), "{xml}");
        assert!(xml.contains("<updated>2025-05-05T00:00:00.000Z</updated>"));
        let (status, _, _) = get_text(&app, &format!("{path}/books/{old}/cover")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "no cover file");
        let (status, _, _) = get_text(&app, "/feeds/shelf/not-a-key").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Opening the shelf is looking at it: the page still says what was
        // new, and after that nothing is.
        let (status, detail) = send(&app, Method::GET, "/api/public/alice/Deckare%20&%20annat", Some(&bob_cookie), None).await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        assert_eq!(detail["follow"]["new_books"], 1);
        assert_eq!(detail["follow"]["seen_at"], "2020-01-01T00:00:00.000Z");
        let (_, news) = send(&app, Method::GET, "/api/follows/news", Some(&bob_cookie), None).await;
        assert_eq!(news.as_array().unwrap().len(), 0);

        // Seen all at once.
        sqlx::query("UPDATE shelf_follows SET seen_at = '2020-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
        let (status, _) = send(&app, Method::POST, "/api/follows/seen", Some(&bob_cookie), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let (_, news) = send(&app, Method::GET, "/api/follows/news", Some(&bob_cookie), None).await;
        assert_eq!(news.as_array().unwrap().len(), 0);

        // No longer shared: no news and no feed. Shared again: both back.
        sqlx::query("UPDATE shelf_follows SET seen_at = '2020-01-01T00:00:00.000Z'").execute(&db).await.unwrap();
        sqlx::query("UPDATE shelves SET visibility = 'private' WHERE id = ?").bind(shelf).execute(&db).await.unwrap();
        let (_, news) = send(&app, Method::GET, "/api/follows/news", Some(&bob_cookie), None).await;
        assert_eq!(news.as_array().unwrap().len(), 0);
        assert_eq!(get_text(&app, path).await.0, StatusCode::NOT_FOUND);
        sqlx::query("UPDATE shelves SET visibility = 'instance' WHERE id = ?").bind(shelf).execute(&db).await.unwrap();
        assert_eq!(get_text(&app, path).await.0, StatusCode::OK);

        // Unfollowed: the feed stops.
        let (status, _) = send(&app, Method::DELETE, &format!("/api/public/shelves/{shelf}/follow"), Some(&bob_cookie), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(get_text(&app, path).await.0, StatusCode::NOT_FOUND);
        let (_, list) = send(&app, Method::GET, "/api/public/shelves", Some(&bob_cookie), None).await;
        assert_eq!(list[0]["follow"]["followed"], false);
    }
}
