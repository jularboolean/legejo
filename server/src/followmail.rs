//! Mail about followed shelves. A follower with a confirmed address is told
//! by mail what has been put on the shelves they follow since they last
//! looked or were last told: one message at most a day, sent once the first
//! new book has been on its shelf for an hour, so that an owner filling a
//! shelf gives one mail and not one per book. Each follower can turn it off
//! on the account page; the mail says where.
//!
//! Two server processes running side by side (a rolling update) do not mail
//! twice: a follower is claimed before the mail goes, by moving their
//! `follow_mailed_at` forward in one statement.

use crate::booktext::esc;
use crate::db::now_ts;
use crate::shelves::visible_to;
use crate::AppState;
use std::collections::BTreeMap;
use std::time::Duration;

/// Hours a new book waits before it is mailed about, for more to join it.
const SETTLE_HOURS: i64 = 1;
/// Hours between two mails to the same follower.
const BETWEEN_HOURS: i64 = 24;
/// Titles named per shelf.
const TITLES: i64 = 5;
/// How often the worker looks.
const ROUND: Duration = Duration::from_secs(600);

#[derive(sqlx::FromRow)]
struct News {
    user_id: i64,
    username: String,
    email: String,
    shelf_id: i64,
    shelf: String,
    owner: String,
    new_books: i64,
    /// The first of the new books, as put on the shelf.
    first: String,
}

#[derive(sqlx::FromRow)]
struct Title {
    title: String,
    author: Option<String>,
    uuid: String,
}

pub async fn worker(state: AppState) {
    loop {
        tokio::time::sleep(ROUND).await;
        if let Err(e) = round(&state).await {
            tracing::warn!("follow mail: {e:#}");
        }
    }
}

/// Mail every follower who is due a mail. Returns how many were sent.
pub(crate) async fn round(state: &AppState) -> anyhow::Result<usize> {
    if state.live.mail().is_none() {
        return Ok(0);
    }
    let due_before = crate::db::ts_in_hours(-BETWEEN_HOURS);
    let rows: Vec<News> = sqlx::query_as(&format!(
        "SELECT u.id AS user_id, u.username, u.email, s.id AS shelf_id, s.name AS shelf, o.username AS owner,
                COUNT(sb.book_id) AS new_books, MIN(sb.added_at) AS first
         FROM shelf_follows f
         JOIN users u ON u.id = f.user_id
         JOIN shelves s ON s.id = f.shelf_id
         JOIN users o ON o.id = s.owner_id
         JOIN shelf_books sb ON sb.shelf_id = s.id
              AND sb.added_at > (CASE WHEN f.mailed_at IS NOT NULL AND f.mailed_at > f.seen_at THEN f.mailed_at ELSE f.seen_at END)
         WHERE u.follow_mail <> 0 AND u.email IS NOT NULL AND u.verify_token IS NULL
           AND (u.follow_mailed_at IS NULL OR u.follow_mailed_at < $1)
           AND {visible}
         GROUP BY u.id, u.username, u.email, s.id, s.name, o.username
         ORDER BY u.id, MAX(sb.added_at) DESC",
        visible = visible_to("f.user_id"),
    ))
    .bind(&due_before)
    .fetch_all(&state.db)
    .await?;
    let mut by_user: BTreeMap<i64, Vec<News>> = BTreeMap::new();
    for row in rows {
        by_user.entry(row.user_id).or_default().push(row);
    }
    let settled = crate::db::ts_in_hours(-SETTLE_HOURS);
    let mut sent = 0;
    for (user_id, news) in by_user {
        if !news.iter().any(|n| n.first < settled) {
            continue;
        }
        // Claimed by this process, or already by another.
        let now = now_ts();
        let claimed = sqlx::query(
            "UPDATE users SET follow_mailed_at = $1 WHERE id = $2 AND (follow_mailed_at IS NULL OR follow_mailed_at < $3)",
        )
        .bind(&now)
        .bind(user_id)
        .bind(&due_before)
        .execute(&state.db)
        .await?;
        if claimed.rows_affected() == 0 {
            continue;
        }
        let mut shelves = Vec::with_capacity(news.len());
        for n in &news {
            let titles: Vec<Title> = sqlx::query_as(
                "SELECT b.title, b.author, b.uuid FROM books b JOIN shelf_books sb ON sb.book_id = b.id
                 WHERE sb.shelf_id = $1 AND sb.added_at >= $2 ORDER BY sb.added_at DESC, b.id DESC LIMIT $3",
            )
            .bind(n.shelf_id)
            .bind(&n.first)
            .bind(TITLES)
            .fetch_all(&state.db)
            .await?;
            shelves.push((n, titles));
        }
        let (subject, html) = message(state.live.public_url().as_deref(), &news[0].username, &shelves);
        match crate::register::send_mail(state, &news[0].email, &subject, &html).await {
            Ok(()) => {
                for n in &news {
                    sqlx::query("UPDATE shelf_follows SET mailed_at = $1 WHERE user_id = $2 AND shelf_id = $3")
                        .bind(&now)
                        .bind(user_id)
                        .bind(n.shelf_id)
                        .execute(&state.db)
                        .await?;
                }
                sent += 1;
            }
            // Tried again tomorrow; what was new stays new until then.
            Err(e) => tracing::warn!("follow mail to user {user_id} failed: {e:#}"),
        }
    }
    if sent > 0 {
        tracing::info!("follow mail: {sent} sent");
    }
    Ok(sent)
}

/// The subject and the HTML of a mail. Links only when the instance knows
/// its public address.
fn message(base: Option<&str>, username: &str, shelves: &[(&News, Vec<Title>)]) -> (String, String) {
    let subject = match shelves {
        [(only, _)] => format!("New on \u{201c}{}\u{201d}", only.shelf),
        _ => "New on the shelves you follow".to_string(),
    };
    let mut html = format!("<p>Hello {},</p>", esc(username));
    for (n, titles) in shelves {
        let heading = format!("\u{201c}{}\u{201d} by {}", esc(&n.shelf), esc(&n.owner));
        let heading = match base {
            Some(base) => format!("<a href=\"{}\">{heading}</a>", esc(&crate::follows::shelf_url(base, &n.owner, &n.shelf, None))),
            None => heading,
        };
        let count = if n.new_books == 1 { "1 new book".to_string() } else { format!("{} new books", n.new_books) };
        html.push_str(&format!("<p><strong>{heading}</strong>: {count}</p><ul>"));
        for t in titles {
            let mut line = esc(&t.title);
            if let Some(base) = base {
                let url = crate::follows::shelf_url(base, &n.owner, &n.shelf, Some(&t.uuid));
                line = format!("<a href=\"{}\">{line}</a>", esc(&url));
            }
            if let Some(author) = t.author.as_deref().filter(|a| !a.trim().is_empty()) {
                line.push_str(&format!(" \u{2014} {}", esc(author)));
            }
            html.push_str(&format!("<li>{line}</li>"));
        }
        let more = n.new_books - titles.len() as i64;
        if more > 0 {
            html.push_str(&format!("<li>and {more} more</li>"));
        }
        html.push_str("</ul>");
    }
    let account = match base {
        Some(base) => format!("<a href=\"{}/account?tab=options\">your account page</a>", esc(base)),
        None => "your account page".to_string(),
    };
    html.push_str(&format!(
        "<p style=\"color:#666\">You get this mail because you follow these shelves in Legejo, at most once a day. \
         Turn it off on {account}.</p>"
    ));
    (subject, html)
}
