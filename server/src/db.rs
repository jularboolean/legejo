use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use sqlx::any::AnyPoolOptions;
use sqlx::migrate::Migrator;
use sqlx::{AnyPool, Executor};
use std::path::Path;
use std::sync::LazyLock;

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

// A database migrated by a newer version is accepted: migrations only add,
// so an older server can run on it after a rollback.
pub static SQLITE_MIGRATOR: LazyLock<Migrator> = LazyLock::new(|| {
    let mut migrator = sqlx::migrate!("./migrations/sqlite");
    migrator.set_ignore_missing(true);
    migrator
});
pub static POSTGRES_MIGRATOR: LazyLock<Migrator> = LazyLock::new(|| {
    let mut migrator = sqlx::migrate!("./migrations/postgres");
    migrator.set_ignore_missing(true);
    migrator
});

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

/// Argon2 takes tens of milliseconds of CPU per call, so hashing and
/// verifying run on the blocking pool: a burst of logins must not stall the
/// worker threads that serve everything else.
pub async fn hash_password(password: &str) -> anyhow::Result<String> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_blocking(&password))
        .await
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))?
}

fn hash_blocking(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("password hashing failed: {e}"))?;
    Ok(hash.to_string())
}

/// Whether the password is the one behind the stored hash. Err only for a
/// hash that cannot be read, which is a broken row, not a wrong password.
pub async fn verify_password(hash: &str, password: &str) -> anyhow::Result<bool> {
    let (hash, password) = (hash.to_string(), password.to_string());
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash).map_err(|e| anyhow::anyhow!("unreadable password hash: {e}"))?;
        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
    .map_err(|e| anyhow::anyhow!("password verification failed: {e}"))?
}

/// A hash with the same parameters as every stored one, made once. A login
/// for a name that does not exist verifies against it, so that the answer
/// takes as long as a wrong password for a real account: the time taken must
/// not tell the two apart.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| hash_blocking("no such account").unwrap_or_default());

pub async fn verify_dummy(password: &str) {
    let password = password.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&DUMMY_HASH).ok()?;
        Argon2::default().verify_password(password.as_bytes(), &parsed).ok()
    })
    .await;
}

/// If no users exist and LEGEJO_ADMIN_PASSWORD is set, create the initial
/// admin account from it (and LEGEJO_ADMIN_USER, "admin" when unset).
/// Without the password the first visitor creates it in the browser
/// (setup.rs). Returns the new admin's id when one was created.
pub async fn bootstrap_admin(pool: &AnyPool) -> anyhow::Result<Option<i64>> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(None);
    }

    let Some(password) = crate::settings::var("LEGEJO_ADMIN_PASSWORD")? else {
        println!("\n==============================================");
        println!("  No account yet: open Legejo in a browser");
        println!("  and create the admin account there.");
        println!("==============================================\n");
        return Ok(None);
    };
    let username = crate::settings::var("LEGEJO_ADMIN_USER")?.unwrap_or_else(|| "admin".into());
    let hash = hash_password(&password).await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO users (username, password_hash, is_admin) VALUES ($1, $2, 1) RETURNING id")
        .bind(&username)
        .bind(&hash)
        .fetch_one(pool)
        .await?;
    tracing::info!("bootstrap: admin account '{username}' created from env");
    Ok(Some(id))
}

