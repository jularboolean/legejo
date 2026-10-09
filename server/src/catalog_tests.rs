//! OPDS catalogs of other libraries: reading their feeds, and the way from
//! an added catalog to a book in the library.

use crate::catalogs::{fill_template, opensearch_template, parse_feed, SearchLink};
use crate::library_tests::epub;
use crate::testutil::{add_user, send, test_app, test_app_private};
use axum::extract::Query;
use axum::http::{header, Method, StatusCode};
use axum::routing::get;
use axum::Router;
use reqwest::Url;
use serde_json::json;
use std::collections::HashMap;

fn base() -> Url {
    Url::parse("https://books.example/opds/root.xml").unwrap()
}

const NAVIGATION: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <title>Example  Library</title>
  <link rel="search" type="application/opensearchdescription+xml" href="/osd.xml"/>
  <link rel="self" type="application/atom+xml;profile=opds-catalog" href="/opds/root.xml"/>
  <link rel="next" type="application/atom+xml;profile=opds-catalog" href="root.xml?page=2&amp;x=1"/>
  <entry>
    <title>Popular</title>
    <content type="text">The most fetched books.</content>
    <link type="application/atom+xml;profile=opds-catalog" rel="subsection" href="/opds/popular"/>
    <link type="image/png" rel="http://opds-spec.org/image/thumbnail" href="data:image/png;base64,AAAA"/>
  </entry>
</feed>"#;

const BOOKS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dcterms="http://purl.org/dc/terms/">
  <title>A book</title>
  <entry>
    <title>Pride and Prejudice</title>
    <author><name>Austen, Jane</name></author>
    <dcterms:language>en</dcterms:language>
    <rights>Public domain in the USA.</rights>
    <content type="xhtml"><div xmlns="http://www.w3.org/1999/xhtml">
      <p>Title: Pride and Prejudice</p>
      <p>
Summary:
A novel of <i>manners</i>, published in 1813.
      </p>
      <p>Downloads: 191414</p>
    </div></content>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition" title="EPUB3" length="561102" href="http://books.example/ebooks/1342.epub3"/>
    <link type="application/x-mobipocket-ebook" rel="http://opds-spec.org/acquisition" href="/ebooks/1342.kindle"/>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition/buy" href="/buy/1342"/>
    <link type="application/pdf" rel="http://opds-spec.org/acquisition/open-access" href="/ebooks/1342.pdf"/>
    <link type="image/jpeg" rel="http://opds-spec.org/image" href="/covers/1342.medium.jpg"/>
    <link type="image/jpeg" rel="http://opds-spec.org/image/thumbnail" href="/covers/1342.small.jpg"/>
    <link type="application/atom+xml;profile=opds-catalog" rel="related" href="/opds/author/68"/>
  </entry>
  <entry>
    <title>Only for sale</title>
    <summary type="html">&lt;p&gt;Not &lt;b&gt;free&lt;/b&gt; &amp;amp; not here.&lt;/p&gt;</summary>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition/buy" href="/buy/2"/>
  </entry>
</feed>"#;

#[test]
fn a_navigation_feed_is_sections_to_go_into() {
    let feed = parse_feed(NAVIGATION.as_bytes(), &base()).unwrap();
    assert_eq!(feed.title, "Example Library");
    assert_eq!(feed.next.as_deref(), Some("https://books.example/opds/root.xml?page=2&x=1"));
    assert_eq!(feed.search, Some(SearchLink { href: "https://books.example/osd.xml".into(), template: false }));
    let popular = &feed.entries[0];
    assert_eq!(popular.title, "Popular");
    assert_eq!(popular.summary.as_deref(), Some("The most fetched books."));
    assert_eq!(popular.href.as_deref(), Some("https://books.example/opds/popular"));
    assert!(popular.files.is_empty());
    // An icon written into the feed itself is not a cover to fetch.
    assert_eq!(popular.cover, None);
}

#[test]
fn a_book_is_the_files_that_are_handed_over_freely() {
    let feed = parse_feed(BOOKS.as_bytes(), &base()).unwrap();
    let book = &feed.entries[0];
    assert_eq!(book.authors, ["Austen, Jane"]);
    assert_eq!(book.language.as_deref(), Some("en"));
    assert_eq!(book.rights.as_deref(), Some("Public domain in the USA."));
    // The paragraph that begins "Summary:" out of a whole record.
    assert_eq!(book.summary.as_deref(), Some("A novel of manners , published in 1813."));
    // EPUB and PDF; not the Kindle file, and not the one that is for sale.
    let files: Vec<_> = book.files.iter().map(|f| (f.format, f.href.as_str())).collect();
    assert_eq!(
        files,
        [("epub", "https://books.example/ebooks/1342.epub3"), ("pdf", "https://books.example/ebooks/1342.pdf")],
        "an http link of an https catalog is read as https"
    );
    assert_eq!(book.files[0].title.as_deref(), Some("EPUB3"));
    assert_eq!(book.files[0].size, Some(561102));
    assert_eq!(book.cover.as_deref(), Some("https://books.example/covers/1342.small.jpg"));
    // A link to the author's other books is not where the entry leads.
    assert_eq!(book.href, None);

    let sale = &feed.entries[1];
    assert!(sale.files.is_empty());
    assert_eq!(sale.summary.as_deref(), Some("Not free & not here."));
}

#[test]
fn what_is_not_a_feed_is_refused() {
    assert!(parse_feed(b"<html><body>Log in</body></html>", &base()).is_err());
    assert!(parse_feed(b"not xml at all", &base()).is_err());
    assert!(parse_feed(b"", &base()).is_err());
}

#[test]
fn the_search_address_is_read_from_the_description_and_filled_in() {
    let osd = br#"<?xml version="1.0"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <Url type="text/html" template="http://books.example/search/?query={searchTerms}"/>
  <Url type="application/atom+xml" template="/search.opds/?query={searchTerms}&amp;page={startPage?}"/>
</OpenSearchDescription>"#;
    let template = opensearch_template(osd, &base()).unwrap();
    assert_eq!(template, "https://books.example/search.opds/?query={searchTerms}&page={startPage?}");
    assert_eq!(fill_template(&template, "röda rummet & co"), "https://books.example/search.opds/?query=r%C3%B6da%20rummet%20%26%20co&page=");
    assert_eq!(fill_template("https://b.example/s/{searchTerms}/{count}/{startIndex}", "a/b"), "https://b.example/s/a%2Fb/25/1");
}

/// Catalogs turned on for the instance by an admin, and by each of `users`
/// for themselves.
async fn turn_on(app: &Router, db: &sqlx::AnyPool, users: &[&str]) {
    sqlx::query("UPDATE users SET is_admin = 1 WHERE username = 'alice'").execute(db).await.unwrap();
    let alice = format!("{}=session-alice", crate::auth::SESSION_COOKIE);
    let body = json!({ "libris_enabled": true, "registration_enabled": false, "catalogs_enabled": true });
    let (status, v) = send(app, Method::PUT, "/api/admin/settings", Some(&alice), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["catalogs_enabled"], true);
    for name in users {
        let cookie = format!("{}=session-{name}", crate::auth::SESSION_COOKIE);
        let (status, v) = send(app, Method::PUT, "/api/account/catalogs", Some(&cookie), Some(json!({ "enabled": true }))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["catalogs"], true);
    }
}

#[tokio::test]
async fn catalogs_are_off_until_an_admin_and_then_the_user_turn_them_on() {
    let (app, db, _) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    let (_, bob) = add_user(&db, "bob").await;
    let on = |v: &serde_json::Value| v["catalogs"].as_bool().unwrap();

    // Off from the start: nothing to see, nothing to turn on.
    let (status, _) = send(&app, Method::GET, "/api/catalogs", Some(&alice), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, v) = send(&app, Method::GET, "/api/account", Some(&alice), None).await;
    assert_eq!(v["catalogs_available"], false);
    let (_, v) = send(&app, Method::GET, "/api/config", Some(&alice), None).await;
    assert!(!on(&v));

    // A user who turns them on before the admin has still has none.
    let (_, v) = send(&app, Method::PUT, "/api/account/catalogs", Some(&bob), Some(json!({ "enabled": true }))).await;
    assert_eq!(v["catalogs_available"], false);
    let (status, _) = send(&app, Method::GET, "/api/catalogs", Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The admin turns them on: Alice has, for herself; Bob had already.
    turn_on(&app, &db, &["alice"]).await;
    let (status, _) = send(&app, Method::GET, "/api/catalogs", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, v) = send(&app, Method::GET, "/api/config", Some(&bob), None).await;
    assert!(on(&v));

    // Each user turns them off for themselves alone.
    send(&app, Method::PUT, "/api/account/catalogs", Some(&bob), Some(json!({ "enabled": false }))).await;
    let (status, _) = send(&app, Method::POST, "/api/catalogs", Some(&bob), Some(json!({ "url": "https://books.example/opds" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::GET, "/api/catalogs", Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
}

/// A catalog on this machine: a first page with a search and one section,
/// the section with one book, and the book's file.
async fn catalog_server() -> String {
    async fn root() -> impl axum::response::IntoResponse {
        (
            [(header::CONTENT_TYPE, "application/atom+xml;profile=opds-catalog")],
            r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Fria böcker</title>
  <link rel="search" type="application/opensearchdescription+xml" href="/osd.xml"/>
  <entry><title>Alla</title><link type="application/atom+xml" rel="subsection" href="/opds/all"/></entry>
</feed>"#,
        )
    }
    async fn osd() -> &'static str {
        r#"<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <Url type="application/atom+xml" template="/opds/all?q={searchTerms}"/>
</OpenSearchDescription>"#
    }
    async fn all(Query(q): Query<HashMap<String, String>>) -> String {
        let title = match q.get("q") {
            Some(q) => format!("Sökning: {q}"),
            None => "Alla".to_string(),
        };
        format!(
            r#"<feed xmlns="http://www.w3.org/2005/Atom">
  <title>{title}</title>
  <entry>
    <title>Röda rummet</title>
    <author><name>August Strindberg</name></author>
    <link type="application/epub+zip" rel="http://opds-spec.org/acquisition/open-access" href="/files/rr.epub"/>
    <link type="image/png" rel="http://opds-spec.org/image/thumbnail" href="/covers/rr.png"/>
  </entry>
</feed>"#
        )
    }
    async fn file() -> Vec<u8> {
        epub("Röda rummet", "urn:uuid:rr")
    }
    async fn cover() -> impl axum::response::IntoResponse {
        ([(header::CONTENT_TYPE, "image/png")], vec![0x89, b'P', b'N', b'G'])
    }
    async fn svg() -> impl axum::response::IntoResponse {
        ([(header::CONTENT_TYPE, "image/svg+xml")], "<svg xmlns='http://www.w3.org/2000/svg'/>")
    }
    let app = Router::new()
        .route("/opds", get(root))
        .route("/osd.xml", get(osd))
        .route("/opds/all", get(all))
        .route("/files/rr.epub", get(file))
        .route("/covers/rr.png", get(cover))
        .route("/covers/rr.svg", get(svg))
        .route(
            "/opds/slow",
            get(|| async {
                // Fails the first time it is asked, the way a search that the
                // catalog has not made before can.
                static ASKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                if ASKED.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    (StatusCode::GATEWAY_TIMEOUT, String::new())
                } else {
                    (StatusCode::OK, r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Till slut</title></feed>"#.to_string())
                }
            }),
        )
        .route("/opds/down", get(|| async { StatusCode::BAD_GATEWAY }))
        .route("/opds/denied", get(|| async { (StatusCode::FORBIDDEN, "<html><body>Error 403</body></html>") }))
        .route("/page.html", get(|| async { "<html><body>Hello</body></html>" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn from_an_added_catalog_to_a_book_in_the_library() {
    let there = catalog_server().await;
    let (app, db, _) = test_app_private().await;
    let (_, alice) = add_user(&db, "alice").await;
    let (_, bob) = add_user(&db, "bob").await;
    turn_on(&app, &db, &["alice", "bob"]).await;

    // Adding: the title is the catalog's own, and it can be searched.
    let (status, v) = send(&app, Method::POST, "/api/catalogs", Some(&alice), Some(json!({ "url": format!("{there}/opds") }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["title"], "Fria böcker");
    assert_eq!(v["searchable"], true);
    let id = v["id"].as_i64().unwrap();
    let (status, v) = send(&app, Method::POST, "/api/catalogs", Some(&alice), Some(json!({ "url": format!("{there}/opds") }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["id"], id);

    // A page that is not a catalog is not added.
    let (status, v) = send(&app, Method::POST, "/api/catalogs", Some(&alice), Some(json!({ "url": format!("{there}/page.html") }))).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(v["error"], "not a catalog");

    // It is Alice's alone.
    let (_, v) = send(&app, Method::GET, "/api/catalogs", Some(&bob), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
    let (status, _) = send(&app, Method::GET, &format!("/api/catalogs/{id}/feed"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&app, Method::DELETE, &format!("/api/catalogs/{id}"), Some(&bob), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The first page, then the section it leads to.
    let (status, v) = send(&app, Method::GET, &format!("/api/catalogs/{id}/feed"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["catalog"]["title"], "Fria böcker");
    let section = v["entries"][0]["href"].as_str().unwrap().to_string();
    assert_eq!(section, format!("{there}/opds/all"));
    let uri = format!("/api/catalogs/{id}/feed?url={}", url::form_urlencoded::byte_serialize(section.as_bytes()).collect::<String>());
    let (_, v) = send(&app, Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(v["title"], "Alla");
    let book = &v["entries"][0];
    assert_eq!(book["authors"][0], "August Strindberg");
    assert_eq!(book["files"][0]["format"], "epub");
    let href = book["files"][0]["href"].as_str().unwrap().to_string();
    let cover = book["cover"].as_str().unwrap().to_string();

    // A page the catalog refuses, or does not have, is told apart from a
    // catalog that does not answer.
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    for (path, error) in [("/opds/denied", "refused"), ("/opds/nowhere", "missing"), ("/opds/down", "busy")] {
        let uri = format!("/api/catalogs/{id}/feed?url={}", enc(&format!("{there}{path}")));
        let (status, v) = send(&app, Method::GET, &uri, Some(&alice), None).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(v["error"], error, "{path}");
    }

    // A gateway error is asked about once more before it is given up on.
    let uri = format!("/api/catalogs/{id}/feed?url={}", enc(&format!("{there}/opds/slow")));
    let (status, v) = send(&app, Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["title"], "Till slut");

    // Searching goes to the address the catalog's description gave.
    let (_, v) = send(&app, Method::GET, &format!("/api/catalogs/{id}/feed?q=r%C3%B6da%20rummet"), Some(&alice), None).await;
    assert_eq!(v["title"], "Sökning: röda rummet");

    // The cover comes by way of this server; an SVG does not.
    let (status, _) = send(&app, Method::GET, &format!("/api/catalogs/{id}/image?url={}", enc(&cover)), Some(&alice), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&app, Method::GET, &format!("/api/catalogs/{id}/image?url={}", enc(&format!("{there}/covers/rr.svg"))), Some(&alice), None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    // Fetching the book: an ordinary book, named by its own file, and logged.
    let body = json!({ "href": href, "title": "Röda rummet", "format": "epub" });
    let (status, v) = send(&app, Method::POST, &format!("/api/catalogs/{id}/import"), Some(&alice), Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(v["title"], "Röda rummet");
    let book_id = v["id"].as_i64().unwrap();
    let logged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM activity_log WHERE action = 'book.catalog_imported'").fetch_one(&db).await.unwrap();
    assert_eq!(logged, 1);

    // The same file again is not a second book.
    let (status, v) = send(&app, Method::POST, &format!("/api/catalogs/{id}/import"), Some(&alice), Some(body)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["book_id"], book_id);

    // A page that is not a book is not stored.
    let body = json!({ "href": format!("{there}/page.html"), "title": "Hello", "format": "epub" });
    let (status, _) = send(&app, Method::POST, &format!("/api/catalogs/{id}/import"), Some(&alice), Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _) = send(&app, Method::DELETE, &format!("/api/catalogs/{id}"), Some(&alice), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, v) = send(&app, Method::GET, "/api/catalogs", Some(&alice), None).await;
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn an_address_inside_the_network_is_never_fetched() {
    let (app, db, _) = test_app().await;
    let (_, alice) = add_user(&db, "alice").await;
    turn_on(&app, &db, &["alice"]).await;
    for url in ["https://10.0.0.5/opds", "http://books.example/opds", "https://localhost/opds", "file:///etc/passwd"] {
        let (status, v) = send(&app, Method::POST, "/api/catalogs", Some(&alice), Some(json!({ "url": url }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{url}");
        assert_eq!(v["error"], "address not allowed", "{url}");
    }
    let (status, _) = send(&app, Method::GET, "/api/catalogs", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
