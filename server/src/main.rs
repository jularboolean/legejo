// The Kobo initialization response is one large json! literal.
#![recursion_limit = "256"]

mod account;
mod admin;
mod app_passwords;
mod oidc;
mod importdir;
mod metrics;
mod ratelimit;
mod settings;
mod audit;
mod auth;
mod books;
mod db;
mod export;
mod fed;
mod invite;
mod kobo;
mod kosync;
mod libris;
mod license;
mod mail;
mod opds;
mod openlibrary;
mod progress;
mod public;
mod pubdate;
mod register;
mod search;
mod shelves;
#[cfg(test)]
mod testutil;
#[cfg(test)]
mod license_tests;
#[cfg(test)]
mod fed_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod oidc_tests;
#[cfg(test)]
mod selfhost_tests;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use sqlx::AnyPool;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: AnyPool,
    pub backend: db::Backend,
    pub data_dir: PathBuf,
    /// Path to the external kepubify binary, when available: enables KEPUB
    /// downloads for Kobo sync. Overridable via LEGEJO_KEPUBIFY.
    pub kepubify: Option<PathBuf>,
    /// Shared client for outgoing requests (Libris lookups).
    pub http: reqwest::Client,
    /// SMTP settings from the environment; None = mail (and thereby
    /// registration) is unavailable.
    pub mail: Option<mail::MailConfig>,
    /// Limits concurrent cover decoding for thumbnails (memory).
    pub thumb_gate: std::sync::Arc<tokio::sync::Semaphore>,
    /// Federation: config, SSRF-safe client, worker wake-up.
    pub fed: std::sync::Arc<fed::Fed>,
    /// Pokes the export worker when an export is queued.
    pub export_wake: std::sync::Arc<tokio::sync::Notify>,
    /// Optional OIDC login (LEGEJO_OIDC_*); None = only the password form.
    pub oidc: Option<std::sync::Arc<oidc::Oidc>>,
    /// Operator settings from the environment (settings.rs).
    pub settings: std::sync::Arc<settings::Settings>,
}

fn find_kepubify() -> Option<PathBuf> {
    // Set but empty means explicitly disabled.
    if let Ok(path) = std::env::var("LEGEJO_KEPUBIFY") {
        if path.is_empty() {
            return None;
        }
        return Some(PathBuf::from(path));
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("kepubify"))
            .find(|p| p.is_file())
    })
}

/// Unknown paths under /api are a 404, never the SPA's index.html: OPDS
/// readers and scripts must not get an HTML page with status 200.
async fn api_not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/auth/login",
            post(auth::login).layer(axum::middleware::from_fn_with_state(state.clone(), ratelimit::login)),
        )
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/config", get(admin::config))
        .route("/admin/settings", get(admin::get_settings).put(admin::update_settings))
        .route("/admin/users", get(admin::list_users))
        .route("/admin/log", get(audit::list))
        .route("/admin/users/{id}/admin", axum::routing::put(admin::set_admin))
        .route("/admin/test-mail", post(admin::test_mail))
        .route("/admin/invites", post(invite::create))
        .route("/admin/invites/{id}", axum::routing::delete(invite::withdraw))
        .route("/admin/invites/{id}/renew", post(invite::renew))
        .route("/invite", get(invite::lookup).post(invite::accept))
        .route("/register", post(register::register))
        .route("/register/enabled", get(register::enabled))
        .route("/register/verify", post(register::verify))
        .route("/register/forgot", post(register::forgot))
        .route("/register/reset", post(register::reset))
        .route("/account", get(account::get).put(account::update).delete(account::delete_account))
        .route("/account/metadata", get(export::metadata))
        .route("/account/app-passwords", get(app_passwords::list).post(app_passwords::create))
        .route("/account/app-passwords/{id}", axum::routing::delete(app_passwords::delete))
        .route(
            "/account/kosync-key",
            get(kosync::get_key).post(kosync::create_key).delete(kosync::delete_key),
        )
        .route("/account/export", get(export::status).post(export::start).delete(export::discard))
        .route("/account/export/{id}/{part}", get(export::download))
        .route("/account/locale", axum::routing::put(account::set_locale))
        .route("/account/avatar", post(account::upload_avatar).delete(account::delete_avatar))
        .route("/users/{id}/avatar", get(account::avatar))
        .route("/auth/config", get(oidc::config))
        .route("/auth/oidc/start", get(oidc::start))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/account/oidc", get(oidc::account_status))
        .route(
            "/account/kobo-token",
            post(account::create_kobo_token).delete(account::delete_kobo_token),
        )
        .route("/books", get(books::list).post(books::upload))
        .route("/libris/search", get(libris::search))
        .route("/libris/summary", get(libris::summary))
        .route("/openlibrary/search", get(openlibrary::search))
        .route("/openlibrary/work", get(openlibrary::work))
        .route("/openlibrary/cover/{id}", get(openlibrary::cover))
        .route("/series/{name}", get(books::series))
        .route("/authors", get(books::authors))
        .route("/books/bulk", post(books::bulk))
        .route("/books/archive", get(books::archive))
        .route("/books/{id}", get(books::get_one).put(books::update).delete(books::delete))
        .route("/books/{id}/cover", get(books::cover).post(books::upload_cover))
        .route("/books/{id}/cover/openlibrary", post(openlibrary::use_cover))
        .route("/books/{id}/file", get(books::download))
        .route("/books/{id}/rating", axum::routing::put(books::set_rating))
        .route("/books/{id}/want", axum::routing::put(books::set_want))
        .route("/books/{id}/kobo-restore", post(books::kobo_restore))
        .route("/books/{id}/progress", get(progress::get).put(progress::put))
        .route("/search", get(search::search))
        .route("/fed/status", get(fed::remote::status))
        .route("/fed/follows", get(fed::remote::list_follows).post(fed::remote::follow).delete(fed::remote::unfollow))
        .route("/fed/books", get(fed::remote::books))
        .route("/fed/import", post(fed::remote::import))
        .route("/admin/federation", get(fed::admin::get_settings).put(fed::admin::update_settings))
        .route("/admin/federation/instances", get(fed::admin::list_instances))
        .route(
            "/admin/federation/instances/{domain}",
            axum::routing::put(fed::admin::set_instance).delete(fed::admin::delete_instance),
        )
        .route("/admin/federation/preview", post(fed::admin::preview))
        .route("/admin/federation/pending", get(fed::requests::pending))
        .route("/admin/federation/requests/{domain}", axum::routing::delete(fed::requests::dismiss))
        .route("/admin/federation/overview", get(fed::admin::overview))
        .route("/public/shelves", get(public::shelves))
        .route("/public/shelves/{id}/cover", get(public::shelf_cover))
        .route("/public/books/{id}/cover", get(public::book_cover))
        .route("/public/books/{id}/import", post(public::import_book))
        .route("/public/{owner}/{shelf}", get(public::shelf))
        .route("/public/{owner}/{shelf}/{uuid}", get(public::shelf_book))
        .route("/shelves", get(shelves::list).post(shelves::create))
        .route(
            "/shelves/{id}",
            get(shelves::get_one).put(shelves::update).delete(shelves::delete),
        )
        .route(
            "/shelves/{id}/cover",
            get(shelves::cover).post(shelves::upload_cover).delete(shelves::delete_cover),
        )
        .fallback(api_not_found)
        .layer(DefaultBodyLimit::max(state.settings.max_upload_bytes));

    let opds = Router::new()
        .fallback(api_not_found)
        .route("/", get(opds::root))
        .route("/search.xml", get(opds::opensearch))
        .route("/books", get(opds::books))
        .route("/books/{id}/cover", get(opds::cover))
        .route("/books/{id}/file", get(opds::download))
        .route("/shelves", get(opds::shelves))
        .route("/shelves/{id}", get(opds::shelf))
        .route("/series", get(opds::series_list))
        .route("/series/{name}", get(opds::series))
        .layer(axum::middleware::from_fn_with_state(state.clone(), ratelimit::basic));

    let kobo = Router::new()
        .route("/v1/initialization", get(kobo::initialization))
        .route("/v1/auth/device", post(kobo::auth_device))
        .route("/v1/library/sync", get(kobo::library_sync))
        .route("/v1/library/{uuid}/metadata", get(kobo::library_metadata))
        .route(
            "/v1/library/{uuid}/state",
            get(kobo::reading_state_get).put(kobo::reading_state_put),
        )
        .route("/v1/library/{uuid}", axum::routing::delete(kobo::library_delete))
        .route("/v1/library/tags", post(kobo::create_tag))
        .route(
            "/v1/library/tags/{tag_id}",
            axum::routing::put(kobo::rename_tag).delete(kobo::delete_tag),
        )
        .route("/v1/library/tags/{tag_id}/items", post(kobo::add_tag_items))
        .route("/v1/library/tags/{tag_id}/items/delete", post(kobo::delete_tag_items))
        .route("/v1/download/{filename}", get(kobo::download))
        .route("/{image_id}/{width}/{height}/{grey}/image.jpg", get(kobo::image))
        .route("/{image_id}/{width}/{height}/{quality}/{grey}/image.jpg", get(kobo::image_quality))
        .fallback(kobo::dummy)
        .layer(axum::middleware::from_fn_with_state(state.clone(), kobo::access_log));

    Router::new()
        .nest("/api", api)
        .nest("/api/opds", opds)
        .nest("/api/kobo/{token}", kobo)
        .nest("/api/kosync", kosync::router())
        .route("/metrics", get(metrics::metrics))
        .merge(fed::routes::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "legejo=debug,tower_http=info".into());
    // LEGEJO_LOG_FORMAT=json emits one JSON object per line for log
    // shippers. ANSI colours only when stdout is a terminal.
    let ansi = std::io::IsTerminal::is_terminal(&std::io::stdout());
    match settings::var_lossy("LEGEJO_LOG_FORMAT").as_deref() {
        Some("json") => tracing_subscriber::fmt().json().with_env_filter(filter).init(),
        _ => tracing_subscriber::fmt().with_ansi(ansi).with_env_filter(filter).init(),
    }
    let settings = std::sync::Arc::new(settings::Settings::from_env()?);

    let data_dir = PathBuf::from(settings::var("LEGEJO_DATA_DIR")?.unwrap_or_else(|| "data".into()));
    std::fs::create_dir_all(&data_dir)?;

    let (db, backend) = db::connect(&data_dir).await?;
    db::bootstrap_admin(&db).await?;

    let kepubify = find_kepubify();
    match &kepubify {
        Some(path) => tracing::info!("kepub conversion enabled via {}", path.display()),
        None => tracing::info!("kepubify not found; Kobo sync serves plain EPUB"),
    }
    let http = reqwest::Client::builder()
        .user_agent(concat!("Legejo/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let mail = mail::from_env()?;
    match &mail {
        Some(_) => tracing::info!("mail configured (LEGEJO_SMTP_HOST)"),
        None => tracing::info!("mail not configured; self-registration unavailable"),
    }
    let thumb_gate = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
    let fed_config = fed::FedConfig::from_env();
    match &fed_config {
        Some(c) => tracing::info!("federation possible as {} (admin setting decides)", c.base),
        None => tracing::info!("LEGEJO_PUBLIC_URL unset; federation unavailable"),
    }
    let fed = fed::Fed::new(fed_config)?;
    let export_wake = std::sync::Arc::new(tokio::sync::Notify::new());
    let oidc = oidc::Oidc::new(oidc::OidcConfig::from_env()?);
    match &oidc {
        Some(o) => tracing::info!("OIDC login enabled via {} ({})", o.config.issuer, o.config.name),
        None => tracing::info!("OIDC login not configured"),
    }
    let l = &settings.limits;
    match (l.max_failures, &l.ip_header) {
        (0, None) => tracing::info!("login throttling off"),
        (n, None) => tracing::info!("login throttling: {n} failures per username in {} min", l.window_minutes),
        (n, Some(h)) => tracing::info!(
            "login throttling: {n} per username+IP, {} per IP or username, in {} min (client IP from {h})",
            l.max_failures_ip,
            l.window_minutes
        ),
    }
    if settings.metrics != settings::Metrics::Off {
        tracing::info!("metrics at /metrics ({})", if matches!(settings.metrics, settings::Metrics::Token(_)) { "bearer token" } else { "open" });
    }
    let state = AppState { db, backend, data_dir, kepubify, http, mail, thumb_gate, fed, export_wake, oidc, settings };
    tokio::spawn(books::backfill_thumbs(state.clone()));
    tokio::spawn(books::backfill_sha256(state.clone()));
    tokio::spawn(pubdate::backfill(state.clone()));
    tokio::spawn(kosync::backfill(state.clone()));
    tokio::spawn(fed::deliver::worker(state.clone()));
    tokio::spawn(audit::prune_daily(state.clone()));
    tokio::spawn(export::worker(state.clone()));
    tokio::spawn(importdir::worker(state.clone()));

    let mut app = router(state);

    // Production: serve the built SvelteKit SPA (adapter-static) from the same
    // port as the API. Unknown paths get index.html so client-side routes
    // survive a reload. Unset in development, where Vite serves the frontend.
    //
    // The hashed files under /_app/immutable never change, but index.html must
    // always be revalidated: a heuristically cached shell keeps referencing
    // chunk hashes that no longer exist after a deploy.
    if let Some(web_dir) = settings::var("LEGEJO_WEB_DIR")?.map(PathBuf::from) {
        use axum::http::{header, HeaderValue};
        use tower_http::set_header::SetResponseHeaderLayer;

        tracing::info!("serving web frontend from {}", web_dir.display());
        let immutable = tower::ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            ))
            .service(ServeDir::new(web_dir.join("_app/immutable")));
        let shell = tower::ServiceBuilder::new()
            .layer(SetResponseHeaderLayer::overriding(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache"),
            ))
            .service(ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html"))));
        app = app
            .nest_service("/_app/immutable", immutable)
            .fallback_service(shell);
    }

    let addr: SocketAddr = settings::var("LEGEJO_ADDR")?
        .unwrap_or_else(|| "127.0.0.1:3000".into())
        .parse()?;
    tracing::info!("listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

/// Resolves on SIGTERM or Ctrl-C. The server then stops accepting connections
/// and lets in-flight requests finish, so a rollout does not cut a download
/// short. The orchestrator's kill timeout bounds the wait.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received, finishing requests in flight");
}
