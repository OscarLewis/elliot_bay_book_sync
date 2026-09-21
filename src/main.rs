use crate::{
    api::init_resources::{Resources, patch_kobo_resources},
    config::AppConfig,
    database::MongoDatabase,
    error::AppError,
    metadata::update_meta::update_metadata,
    scan::scanner::{ScanResponse, run_library_scan},
};
use axum::{
    Json, Router,
    extract::State,
    http::{self, HeaderMap},
    middleware,
    routing::{get, post},
};
use dotenvy::dotenv;
use mongodb::bson::{doc, oid::ObjectId};
use reqwest::StatusCode;
use std::{env, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub mod api;
pub(crate) mod config;
pub(crate) mod database;
pub(crate) mod directories;
pub mod error;
pub mod library;
pub mod logging;
pub(crate) mod metadata;
pub mod scan;

/// Holds shared application state accessible across request handlers
#[derive(Clone)]
pub struct AppState {
    /// Shared application configuration settings.
    pub config: Arc<AppConfig>,
    pub req_client: reqwest::Client,
    pub mongodb: Arc<MongoDatabase>,

    pub hardcover_api_token: Option<String>,
    pub kobo_resources: Arc<Mutex<Resources>>,
    pub patched_resources: Arc<Mutex<Resources>>,
}

impl AppState {
    /// Creates a new `AppState` instance with the given configuration.
    pub fn new(
        config: impl Into<Arc<AppConfig>>,
        mongodb: MongoDatabase,
        hardcover_api_token: Option<String>,
    ) -> Self {
        let config: Arc<AppConfig> = config.into();
        let client = reqwest::Client::builder()
            .user_agent("Kobo Touch/4.38.21908 (Linux 2.6.35.3; U; en-US)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        // Create base resources and apply patches using config values
        let kobo_resources = Resources::default();
        let patched_resources = patch_kobo_resources(
            kobo_resources.clone(),
            &config.base_url,
            &config.ebbooks_auth_key,
            config.proxy_kobo_store,
        );

        AppState {
            mongodb: Arc::new(mongodb),
            config,
            req_client: client,
            hardcover_api_token,
            kobo_resources: Arc::new(Mutex::new(kobo_resources)),
            patched_resources: Arc::new(Mutex::new(patched_resources)),
        }
    }
}

/// Constructs the main application `Router` and registers API routes with shared state
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "Ebook Sync Server" }))
        .route("/scan", post(scan_handler))
        .route("/metadata/refresh", post(refresh_metadata_handler))
        .route(
            "/metadata/refresh/{book_doc_id}",
            post(refresh_single_book_metadata_handler),
        )
        .merge(api::kobo_routes::kobo_routes())
        .with_state(state)
        // .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(logging::log_with_body))
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // Initialize logging subscriber using environment filter (defaults to debug level for kobo_sync_rs)
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("ebbooks=debug,tower_http=info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load config from file and environment
    let config = AppConfig::load()?;
    debug!(?config, "Loaded config");

    // Load the .env file into the system environment
    dotenv().ok();

    // Log an error if there is no Hardcover token for metadata
    let hardcover_api_token = match env::var("HARDCOVER_TOKEN") {
        Ok(val) => {
            debug!("Hardcover Token loaded");
            Some(val)
        }
        Err(e) => {
            error!("Could not find HARDCOVER_TOKEN: {e}");
            None
        }
    };

    // Exit if there is no MongoDB connection URI
    let mongodb_uri = env::var("MONGODB_URI").map_err(|e| {
        error!("Could not find MONGODB_URI: {e}");
        AppError::Internal(format!(
            "Missing required MONGODB_URI environment variable: {e}"
        ))
    })?;
    debug!("MongoDB URI loaded");

    let mongodb = MongoDatabase::connect(&mongodb_uri, &config.mongodb_name.clone()).await?;
    mongodb.db.run_command(doc! { "ping": 1 }).await?;
    info!("MongoDB connection confirmed (ping ok)");

    // Open DB
    // TODO Switch to being backed by MongoDB

    // TODO let scans = mongodb.scans.fetch_all()
    // // Debug all the scans stored in the database
    // let scans: Vec<(String, ScanDocument)> = db.get_all(DocumentTable::Scans)?;
    // debug!(?scans, count = scans.len(), "All stored scans in database");

    // Fetch and debug all stored books & scans from redb
    // let books: Vec<(String, Book)> = db.get_all(DocumentTable::Books)?;
    // debug!(?books, count = books.len(), "All stored books in database");

    // Construct App state
    let state = AppState::new(config, mongodb, hardcover_api_token);

    // Bind state to app
    let app = app(state.clone());

    // Filter through set of all books for those with has_metadata = False
    let books_needing_metadata = state.mongodb.books.find_books_needing_metadata().await?;

    debug!(
        count = books_needing_metadata.len(),
        "Files needing metadata"
    );

    // TODO Move a initial scan into helper function

    if !books_needing_metadata.is_empty() {
        let metadata_state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = update_metadata(metadata_state, books_needing_metadata).await {
                error!(?err, "Metadata update failed");
            }
        });
    }

    // Bind server to local port 3000
    let listener = tokio::net::TcpListener::bind(state.config.bind_addr)
        .await
        .unwrap();

    info!(addr = ?listener.local_addr().unwrap(), "listening");

    // Start serving HTTP requests with Axum
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

/// POST `/scan` request handler
///
/// Generates a unique `scan_id`, launches an asynchronous scan task in the
/// background, and returns the generated UUID to the client immediately
pub async fn scan_handler(State(state): State<AppState>) -> Result<Json<ScanResponse>, AppError> {
    let scan_id = state.mongodb.scans.start().await?;

    debug!(doc_id = ?scan_id, "Initialized scan execution record");

    // Spawn long-running library scanning task asynchronously so handler returns immediately
    tokio::spawn(run_library_scan(
        state.mongodb.clone(),
        scan_id.clone(),
        state.config.library_path.clone(),
    ));

    Ok(Json(ScanResponse {
        scan_id: scan_id.to_string(),
    }))
}

/// Resolves the public base URL that Kobo should use when accessing this server.
///
/// Prefer an explicitly configured external URL when available.
/// Otherwise, use reverse-proxy headers (`X-Forwarded-Host` and `X-Forwarded-Proto`)
/// when present,falling back to the request's `Host` header and finally localhost.  
/// This allows generated Kobo resource URLs to use the externally reachable
/// address even when Axum itself is running behind a reverse proxy.
pub(crate) fn resolve_base_url(headers: &HeaderMap, config_external_url: Option<&str>) -> String {
    if let Some(ext_url) = config_external_url {
        return ext_url.trim_end_matches('/').to_string();
    }

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(http::header::HOST))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:8080");

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|s| s.to_str().ok())
        .unwrap_or("http");

    format!("{scheme}://{host}")
}

pub async fn refresh_metadata_handler(
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    // Filter through set of all books for those with has_metadata = False
    let books_needing_metadata = state.mongodb.books.find_books_needing_metadata().await?;

    let metadata_state = state.clone();

    tokio::spawn(async move {
        if let Err(err) = update_metadata(metadata_state, books_needing_metadata).await {
            error!(?err, "Metadata update failed");
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

pub async fn refresh_single_book_metadata_handler(
    axum::extract::Path(book_doc_id): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let book_id = ObjectId::parse_str(&book_doc_id).map_err(|_| AppError::InvalidObjectId)?;

    let Some(book) = state.mongodb.books.find_by_id(book_id).await? else {
        return Err(AppError::Internal(format!("Book not found: {book_doc_id}")));
    };

    // Reset field "has_metadata" to false before we fetch new metadata
    state.mongodb.books.reset_metadata(book_id).await?;

    let metadata_state = state.clone();

    tokio::spawn(async move {
        if let Err(err) = update_metadata(metadata_state, vec![book]).await {
            error!(?err, "Metadata update failed");
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
pub mod test_helpers {
    use crate::api::init_resources::{Resources, patch_kobo_resources};
    use crate::database::MongoDatabase;
    use crate::error::AppError;
    // adjust to actual module path
    use crate::{AppState, app, config::AppConfig};
    use axum::Router;
    use axum_test::TestServer;
    use dotenvy::dotenv;
    use std::env;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use std::sync::Arc;
    use test_context::AsyncTestContext;
    use tokio::sync::Mutex;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    pub struct AppTestContext {
        pub state: AppState,
        mongodb: MongoDatabase,
    }

    impl AppTestContext {
        pub async fn set_config(&mut self, config: AppConfig) {
            let resources = self.state.kobo_resources.lock().await.clone();

            let patched_resources = patch_kobo_resources(
                resources,
                &config.base_url,
                &config.ebbooks_auth_key,
                config.proxy_kobo_store,
            );

            self.state.config = Arc::new(config);
            *self.state.patched_resources.lock().await = patched_resources;
        }

        pub async fn setup_with_config(config: AppConfig) -> Self {
            dotenv().ok();

            let uri = env::var("MONGODB_TEST_URI")
                .expect("MONGODB_TEST_URI must be set to run tests that require MongoDB");
            let database = format!("ebbooks_test_{}", uuid::Uuid::new_v4());

            let mongodb = MongoDatabase::connect(&uri, &database)
                .await
                .expect("Failed to connect to MongoDB using MONGODB_TEST_URI");

            let kobo_resources = Resources::default();
            let patched_resources = patch_kobo_resources(
                kobo_resources.clone(),
                &config.base_url,
                &config.ebbooks_auth_key,
                config.proxy_kobo_store,
            );

            let state = AppState {
                config: Arc::new(config),
                req_client: reqwest::Client::new(),
                mongodb: Arc::new(mongodb.clone()),
                hardcover_api_token: None,
                kobo_resources: Arc::new(Mutex::new(kobo_resources)),
                patched_resources: Arc::new(Mutex::new(patched_resources)),
            };

            Self { state, mongodb }
        }
    }

    impl AsyncTestContext for AppTestContext {
        async fn setup() -> Self {
            Self::setup_with_config(AppConfig::default()).await
        }

        async fn teardown(self) {
            if let Err(e) = self.mongodb.drop_database().await {
                eprintln!("warning: failed to drop test database: {e}");
            }
        }
    }

    /// Helper utility to bootstrap a `TestServer` instance for integration testing.
    pub fn setup_test_app(state: AppState) -> TestServer {
        let router = Router::new().merge(app(state));
        TestServer::new(router)
    }

    pub fn write_minimal_epub(path: &Path, title: &str) -> Result<(), AppError> {
        let mut zip = ZipWriter::new(File::create(path)?);

        // The mimetype entry must come first and be uncompressed
        let stored_options =
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let default_options = SimpleFileOptions::default();

        zip.start_file("mimetype", stored_options)?;
        zip.write_all(b"application/epub+zip")?;

        zip.start_file("META-INF/container.xml", default_options)?;
        zip.write_all(
            r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
                .as_bytes(),
        )?;

        zip.start_file("OEBPS/content.opf", default_options)?;
        let content_opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:creator>Test Author</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="id">urn:uuid:00000000-0000-0000-0000-000000000001</dc:identifier>
  </metadata>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>"#
        );
        zip.write_all(content_opf.as_bytes())?;

        zip.start_file("OEBPS/chapter1.xhtml", default_options)?;
        zip.write_all(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <title>Ch 1</title>
  </head>
  <body>
    <p>Hello</p>
  </body>
</html>"#
                .as_bytes(),
        )?;

        zip.finish()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        library::book::Book,
        scan::scanner::ScanResponse,
        test_helpers::{AppTestContext, setup_test_app},
    };
    use axum::http::StatusCode;
    use std::path::PathBuf;
    use test_context::test_context;
    use test_log::test;

    /// Tests the root endpoint response.
    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_root_handler(ctx: &mut AppTestContext) {
        let server = setup_test_app(ctx.state.clone());

        let response = server.get("/").await;
        response.assert_status(StatusCode::OK);
    }

    /// Verifies that calling POST `/scan` triggers a background scan and returns a valid UUID.
    #[test_context(AppTestContext)]
    #[test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
    async fn test_scan_handler_triggers_scan(ctx: &mut AppTestContext) {
        let server = setup_test_app(ctx.state.clone());

        let response = server.post("/scan").await;
        response.assert_status(StatusCode::OK);

        let _body: ScanResponse = response.json();
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_refresh_single_book_metadata_handler(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut book = Book::from_path(PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        ));

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let server = setup_test_app(ctx.state.clone());

        let response = server.post(&format!("/metadata/refresh/{book_id}")).await;

        response.assert_status(StatusCode::NO_CONTENT);

        Ok(())
    }
}
