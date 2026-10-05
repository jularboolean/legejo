use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use rand::Rng;
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Executor};
use std::path::Path;

/// Which database the Any pool talks to. All SQL is written in the dialect
/// both engines share ($N placeholders, ON CONFLICT, TEXT timestamps,
/// BIGINT flags); the few real differences (full-text search and the JSON
/// functions in progress.rs) branch on this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    Sqlite,
    Postgres,
}

/// The timestamp format shared with SQLite's strftime('%Y-%m-%dT%H:%M:%fZ'):
/// ISO-8601 with milliseconds, always UTC, lexicographically sortable.
pub(crate) const TS_FORMAT: &[time::format_description::BorrowedFormatItem<'static>] = time::macros::format_description!(
    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
);

pub fn now_ts() -> String {
    time::OffsetDateTime::now_utc().format(&TS_FORMAT).unwrap_or_default()
}

pub fn ts_in_hours(hours: i64) -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::hours(hours))
        .format(&TS_FORMAT)
        .unwrap_or_default()
}

/// now_ts() shifted forward, for session expiry.
pub fn ts_in_days(days: i64) -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::days(days))
        .format(&TS_FORMAT)
        .unwrap_or_default()
}

/// A boolean that travels as BIGINT through the database layer: the sqlx Any
/// driver has no bool mapping that works on both backends. Serializes to a
/// JSON bool, so the API is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct DbFlag(pub i64);

impl DbFlag {
    pub fn as_bool(self) -> bool {
        self.0 != 0
    }
}

impl From<bool> for DbFlag {
    fn from(b: bool) -> Self {
        Self(b as i64)
    }
}

impl serde::Serialize for DbFlag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(self.0 != 0)
    }
}

pub static SQLITE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");
pub static POSTGRES_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/postgres");

/// Connect to DATABASE_URL (postgres://… or sqlite://…); without it, use a
/// SQLite file in the data directory.
pub async fn connect(data_dir: &Path) -> anyhow::Result<(AnyPool, Backend)> {
    sqlx::any::install_default_drivers();

    // DATABASE_URL_FILE works too: the URL holds the database password.
    let url = match crate::settings::var("DATABASE_URL")? {
        Some(url) => url,
        None => format!("sqlite://{}?mode=rwc", data_dir.join("legejo.db").display()),
    };
    let backend = if url.starts_with("postgres") {
        Backend::Postgres
    } else if url.starts_with("sqlite") {
        Backend::Sqlite
    } else {
        anyhow::bail!("DATABASE_URL must start with postgres:// or sqlite://");
    };

    let pool = AnyPoolOptions::new()
        .max_connections(8)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                if backend == Backend::Sqlite {
                    // WAL requires shared memory between processes, so it is
                    // unsafe on network filesystems unless exactly one process
                    // on one host opens the database. Use Postgres for
                    // anything beyond that.
                    conn.execute("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;").await?;
                }
                Ok(())
            })
        })
        .connect(&url)
        .await?;

    match backend {
        Backend::Sqlite => SQLITE_MIGRATOR.run(&pool).await?,
        Backend::Postgres => POSTGRES_MIGRATOR.run(&pool).await?,
    }
    tracing::info!("database ready ({backend:?})");
    Ok((pool, backend))
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))?;
    Ok(hash.to_string())
}

/// If no users exist, create the initial admin account.
/// Username/password come from LEGEJO_ADMIN_USER / LEGEJO_ADMIN_PASSWORD;
/// if no password is set, a random one is generated and printed once.
pub async fn bootstrap_admin(pool: &AnyPool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let username = crate::settings::var("LEGEJO_ADMIN_USER")?.unwrap_or_else(|| "admin".into());
    let (password, generated) = match crate::settings::var("LEGEJO_ADMIN_PASSWORD")? {
        Some(p) => (p, false),
        None => (random_password(16), true),
    };

    let hash = hash_password(&password)?;
    sqlx::query("INSERT INTO users (username, password_hash, is_admin) VALUES ($1, $2, 1)")
        .bind(&username)
        .bind(&hash)
        .execute(pool)
        .await?;

    if generated {
        println!("\n==============================================");
        println!("  Bootstrap: admin account created");
        println!("  username: {username}");
        println!("  password: {password}");
        println!("  (shown only once; change it after login)");
        println!("==============================================\n");
    } else {
        tracing::info!("bootstrap: admin account '{username}' created from env");
    }
    Ok(())
}

fn random_password(len: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect()
}
