//! /f/{slug} and /f/{slug}/{uuid}: plain HTML for people who follow a link
//! from Mastodon or a relay. No login, no JavaScript; same gate as /ap/.

use super::objects::escape;
use super::routes::{fed_book, fed_shelf, on};
use crate::AppState;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};

/// The logo, as in the app (a copy of the web's lib/assets/legejo.svg).
const LOGO: &str = include_str!("legejo.svg");

const REPOSITORY: &str = "https://github.com/jularboolean/legejo";

/// The palette and fonts follow the app's stylesheet (web/src/app.css).
const STYLE: &str = "\
:root{--bg:#f3f5f5;--fg:#1d2224;--muted:#6c7a7e;--accent:#53676c;--card:#fdfefe;--border:#dbe1e2;\
--shadow:0 1px 3px rgba(25,35,38,.1),0 4px 14px rgba(25,35,38,.06)}\
@media (prefers-color-scheme:dark){:root{--bg:#15181a;--fg:#e4e8e9;--muted:#8d9a9e;--accent:#94b0b8;--card:#1d2124;--border:#333a3d;\
--shadow:0 1px 3px rgba(0,0,0,.4),0 4px 14px rgba(0,0,0,.3)}}\
*{box-sizing:border-box}\
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 'Inter Variable',system-ui,sans-serif}\
main{max-width:44rem;margin:0 auto;padding:2rem 1rem}\
h1{font-family:'Fraunces',Georgia,serif;margin:.2rem 0 .4rem}\
a{color:var(--accent)}.muted{color:var(--muted);font-size:.9rem}\
.brand{display:flex;flex-direction:column;align-items:center;gap:.25rem;margin-bottom:1.5rem;padding-bottom:1.25rem;\
border-bottom:1px solid var(--border);color:var(--muted);font-size:.85rem;text-align:center}\
.brand svg{width:auto;height:5.5rem}.brand strong{font-weight:500;letter-spacing:.04em}\
ul{list-style:none;padding:0;display:grid;gap:.75rem}\
li{display:flex;gap:.9rem;background:var(--card);border:1px solid var(--border);border-radius:8px;padding:.7rem;box-shadow:var(--shadow)}\
img{width:4.5rem;aspect-ratio:2/3;object-fit:cover;border-radius:3px;flex-shrink:0}.big img{width:10rem}\
.btn{display:inline-block;margin-top:.6rem;padding:.45rem .9rem;border-radius:6px;background:var(--accent);color:var(--bg);text-decoration:none}";

fn page(title: &str, body: &str) -> Response {
    let logo = LOGO.trim_start_matches(|c| c != '\n').trim_start();
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{t}</title><style>{STYLE}</style></head><body><main>\
         <header class=\"brand\">{logo}<strong>Legejo</strong>\
         <span>Free software for your own library of EPUB books. <a href=\"{REPOSITORY}\">Get it on GitHub</a>.</span></header>\
         {body}</main></body></html>",
        t = escape(title)
    ))
    .into_response()
}

fn licence_text(license: &str, died: Option<i64>) -> String {
    let name = match license {
        "pd" => "Public domain".to_string(),
        "cc0" => "CC0".to_string(),
        l if l.starts_with("cc-") => format!("CC {}", l[3..].to_uppercase()),
        l => l.to_string(),
    };
    match died {
        Some(y) if license == "pd" => format!("{name} (the author died in {y})"),
        _ => name,
    }
}

pub async fn shelf(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Response, Response> {
    let c = on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    let books = super::reconcile::shelf_books(&state, shelf.id).await.unwrap_or_default();
    let mut items = String::new();
    for p in &books {
        let b = &p.book;
        items.push_str(&format!(
            "<li><img src=\"/ap/books/{u}/cover\" alt=\"\"><div><a href=\"/f/{s}/{u}\"><strong>{t}</strong></a><br>{a}\
             {d}</div></li>",
            u = b.uuid,
            s = shelf.ap_slug,
            t = escape(&b.title),
            a = escape(b.author.as_deref().unwrap_or("")),
            d = b
                .description
                .as_deref()
                .map(|d| super::objects::shorten(d, 240))
                .filter(|d| !d.is_empty())
                .map(|d| format!("<div class=\"muted\">{}</div>", escape(&d)))
                .unwrap_or_default(),
        ));
    }
    let handle = format!("@{}@{}", shelf.ap_slug, c.host);
    let body = format!(
        "<p class=\"muted\">A federated shelf on Legejo · {owner}</p><h1>{name}</h1>\
         <p class=\"muted\">Follow from Legejo or Mastodon: <code>{handle}</code></p>{desc}\
         <p class=\"muted\">{n} free books</p><ul>{items}</ul>",
        owner = escape(&shelf.owner),
        name = escape(&shelf.name),
        handle = escape(&handle),
        desc = shelf.description.as_deref().map(super::objects::html_paragraphs).unwrap_or_default(),
        n = books.len(),
    );
    Ok(page(&shelf.name, &body))
}

pub async fn book(State(state): State<AppState>, Path((slug, uuid)): Path<(String, String)>) -> Result<Response, Response> {
    on(&state).await?;
    let shelf = fed_shelf(&state, &slug).await?;
    let b = fed_book(&state, &uuid).await?;
    let on_shelf: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelf_books WHERE shelf_id = $1 AND book_id = $2")
        .bind(shelf.id)
        .bind(b.book.id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    if on_shelf == 0 {
        return Err(super::routes::not_found());
    }
    let book = &b.book;
    let source = book
        .license_source_url
        .as_deref()
        .map(|u| format!(" · <a href=\"{}\" rel=\"noreferrer\">source</a>", escape(u)))
        .unwrap_or_default();
    let body = format!(
        "<p class=\"muted\"><a href=\"/f/{s}\">{shelf}</a></p>\
         <div class=\"big\" style=\"display:flex;gap:1.2rem;flex-wrap:wrap\"><img src=\"/ap/books/{u}/cover\" alt=\"\">\
         <div><h1>{t}</h1><div>{a}</div><p class=\"muted\">{l}{source}</p>\
         <a class=\"btn\" href=\"/ap/books/{u}/epub\">Download EPUB</a></div></div>{d}",
        s = shelf.ap_slug,
        shelf = escape(&shelf.name),
        u = book.uuid,
        t = escape(&book.title),
        a = escape(book.author.as_deref().unwrap_or("")),
        l = escape(&licence_text(book.license.as_deref().unwrap_or(""), book.author_death_year)),
        d = book.description.as_deref().map(super::objects::html_paragraphs).unwrap_or_default(),
    );
    Ok(page(&book.title, &body))
}
