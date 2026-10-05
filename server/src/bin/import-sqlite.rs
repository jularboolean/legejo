//! One-shot import of an existing SQLite catalog into Postgres.
//!
//! Usage:
//!   DATABASE_URL=postgres://… import-sqlite path/to/legejo.db
//!
//! The target database must contain no users. The tool runs the migrations
//! itself and refuses a database that already has users, so do not start the
//! server against it first (the admin bootstrap would create one).
//! Book and cover files on disk are untouched: point LEGEJO_DATA_DIR at the
//! same directory as before.

use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, PgPool, Row, SqlitePool};

#[derive(FromRow)]
struct User {
    id: i64,
    username: String,
    password_hash: String,
    is_admin: i64,
    kobo_token: Option<String>,
    created_at: String,
}

#[derive(FromRow)]
struct BookRow {
    id: i64,
    uuid: String,
    owner_id: i64,
    title: String,
    author: Option<String>,
    language: Option<String>,
    description: Option<String>,
    publisher: Option<String>,
    published: Option<String>,
    identifier: Option<String>,
    category: Option<String>,
    isbn: Option<String>,
    libris_id: Option<String>,
    file_size: i64,
    cover_mime: Option<String>,
    created_at: String,
    updated_at: Option<String>,
    source_uuid: Option<String>,
    series: Option<String>,
    series_index: Option<f64>,
}

#[derive(FromRow)]
struct ShelfRow {
    id: i64,
    owner_id: i64,
    name: String,
    is_public: i64,
    description: Option<String>,
    cover_mime: Option<String>,
    created_at: String,
    updated_at: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sqlite_path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: import-sqlite <path/to/legejo.db> (target in DATABASE_URL)"))?;
    let pg_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("set DATABASE_URL to the Postgres target"))?;
    if !pg_url.starts_with("postgres") {
        anyhow::bail!("DATABASE_URL must be postgres://…");
    }

    let sqlite: SqlitePool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&sqlite_path).read_only(true))
        .await?;
    let pg: PgPool = PgPoolOptions::new().max_connections(4).connect(&pg_url).await?;

    // Migrate the target, then require it to be empty so we never merge.
    sqlx::migrate!("./migrations/postgres").run(&pg).await?;
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&pg).await?;
    if existing > 0 {
        anyhow::bail!(
            "target database already has {existing} user(s); import only into an empty database"
        );
    }

    let mut tx = pg.begin().await?;

    let users: Vec<User> = sqlx::query_as(
        "SELECT id, username, password_hash, is_admin, kobo_token, created_at FROM users",
    )
    .fetch_all(&sqlite)
    .await?;
    for u in &users {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, kobo_token, created_at)
             OVERRIDING SYSTEM VALUE VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(u.id)
        .bind(&u.username)
        .bind(&u.password_hash)
        .bind(u.is_admin)
        .bind(&u.kobo_token)
        .bind(&u.created_at)
        .execute(&mut *tx)
        .await?;
    }
    println!("users: {}", users.len());

    let books: Vec<BookRow> = sqlx::query_as(
        "SELECT id, uuid, owner_id, title, author, language, description, publisher, published,
                identifier, category, isbn, libris_id, file_size, cover_mime, created_at, updated_at,
                source_uuid, series, series_index
         FROM books",
    )
    .fetch_all(&sqlite)
    .await?;
    for b in &books {
        sqlx::query(
            "INSERT INTO books (id, uuid, owner_id, title, author, language, description, publisher,
                                published, identifier, category, isbn, libris_id, file_size, cover_mime,
                                created_at, updated_at, source_uuid, series, series_index)
             OVERRIDING SYSTEM VALUE
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)",
        )
        .bind(b.id)
        .bind(&b.uuid)
        .bind(b.owner_id)
        .bind(&b.title)
        .bind(&b.author)
        .bind(&b.language)
        .bind(&b.description)
        .bind(&b.publisher)
        .bind(&b.published)
        .bind(&b.identifier)
        .bind(&b.category)
        .bind(&b.isbn)
        .bind(&b.libris_id)
        .bind(b.file_size)
        .bind(&b.cover_mime)
        .bind(&b.created_at)
        .bind(&b.updated_at)
        .bind(&b.source_uuid)
        .bind(&b.series)
        .bind(b.series_index)
        .execute(&mut *tx)
        .await?;
    }
    println!("books: {}", books.len());

    let shelves: Vec<ShelfRow> = sqlx::query_as(
        "SELECT id, owner_id, name, is_public, description, cover_mime, created_at, updated_at FROM shelves",
    )
    .fetch_all(&sqlite)
    .await?;
    for s in &shelves {
        sqlx::query(
            "INSERT INTO shelves (id, owner_id, name, is_public, description, cover_mime, created_at, updated_at)
             OVERRIDING SYSTEM VALUE VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(s.id)
        .bind(s.owner_id)
        .bind(&s.name)
        .bind(s.is_public)
        .bind(&s.description)
        .bind(&s.cover_mime)
        .bind(&s.created_at)
        .bind(&s.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    println!("shelves: {}", shelves.len());

    for (table, columns) in [
        ("sessions", vec!["token", "user_id", "created_at", "expires_at"]),
        ("shelf_books", vec!["shelf_id", "book_id", "added_at"]),
        ("kobo_reading_state", vec!["user_id", "book_id", "state", "updated_at"]),
        ("kobo_deleted_shelves", vec!["shelf_id", "owner_id", "deleted_at"]),
        ("reading_progress", vec!["user_id", "book_id", "cfi", "percent", "updated_at"]),
        ("book_tags", vec!["book_id", "tag"]),
    ] {
        let rows = sqlx::query(&format!("SELECT {} FROM {table}", columns.join(", ")))
            .fetch_all(&sqlite)
            .await?;
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
        let insert = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        );
        for row in &rows {
            let mut query = sqlx::query(&insert);
            for (i, _) in columns.iter().enumerate() {
                // SQLite stores ints, floats and text; try in that order.
                if let Ok(v) = row.try_get::<i64, _>(i) {
                    query = query.bind(v);
                } else if let Ok(v) = row.try_get::<f64, _>(i) {
                    query = query.bind(v);
                } else {
                    query = query.bind(row.try_get::<Option<String>, _>(i)?);
                }
            }
            query.execute(&mut *tx).await?;
        }
        println!("{table}: {}", rows.len());
    }

    // Identity sequences continue after the imported ids.
    for table in ["users", "books", "shelves"] {
        sqlx::query(&format!(
            "SELECT setval(pg_get_serial_sequence('{table}', 'id'), GREATEST((SELECT COALESCE(MAX(id), 0) FROM {table}), 1))"
        ))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    println!("done; point the server at DATABASE_URL and keep LEGEJO_DATA_DIR unchanged");
    Ok(())
}
