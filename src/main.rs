use crate::{
    api::init_resources::{Resources, patch_kobo_resources},
    config::AppConfig,
    database::{
        MongoDatabase,
        document::{DocumentDB, DocumentTable},
    },
    error::AppError,
    library::book::Book,
    metadata::update_meta::update_metadata,
    scan::scanner::{ScanDetails, ScanDocument, ScanResponse, ScanStatus, run_library_scan},
};
use axum::{
    Json, Router,
    extract::State,
    http::{self, HeaderMap},
    middleware,
    routing::{get, post},
};
use chrono::Utc;
use dotenvy::dotenv;
use mongodb::bson::doc;
use reqwest::StatusCode;
use std::sync::Arc;
use std::{env, process::ExitCode};
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

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
    pub db: Arc<DocumentDB>,
    pub mongodb: Arc<MongoDatabase>,

    pub hardcover_api_token: Option<String>,
    pub kobo_resources: Arc<Mutex<Resources>>,
    pub patched_resources: Arc<Mutex<Resources>>,
}

impl AppState {
    /// Creates a new `AppState` instance with the given configuration.
    pub fn new(
        config: impl Into<Arc<AppConfig>>,
        db: DocumentDB,
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
            db: Arc::new(db),
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
    let db = DocumentDB::open(&config.database_path)?;

    // Debug all the scans stored in the database
    let scans: Vec<(String, ScanDocument)> = db.get_all(DocumentTable::Scans)?;
    debug!(?scans, count = scans.len(), "All stored scans in database");

    // Fetch and debug all stored books & scans from redb
    let books: Vec<(String, Book)> = db.get_all(DocumentTable::Books)?;
    debug!(?books, count = books.len(), "All stored books in database");

    // Construct App state
    let state = AppState::new(config, db, mongodb, hardcover_api_token);

    // Bind state to app
    let app = app(state.clone());

    // Filter through set of all books for those with has_metadata = False
    let books_needing_metadata: Vec<(String, Book)> = books
        .into_iter()
        .filter(|(_, book)| !book.has_metadata)
        .collect();

    debug!(
        count = books_needing_metadata.len(),
        "Files needing metadata"
    );

    // TODO Move this initial scan into helper function
    /*
    let initial_record = ScanDocument {
        status: ScanStatus::Running,
        timestamp: Utc::now().to_rfc3339(),
        details: ScanDetails::Started,
    };

    let record_doc_id = state.db.create(
        DocumentTable::Scans,
        &initial_record,
        None,
        Some(|s| s.timestamp.as_str()),
    )?;

    debug!(doc_id = %record_doc_id, "Initialized scan execution record");

    tokio::spawn(run_library_scan(
        state.db.clone(),
        record_doc_id.clone(),
        state.config.library_path.clone(),
    ));
    */

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
        state.db.clone(),
        state.mongodb.clone(),
        scan_id.clone(),
        state.config.library_path.clone(),
    ));

    Ok(Json(ScanResponse {
        scan_id: Uuid::parse_str(&scan_id.to_string().clone())
            .map_err(|e| AppError::Internal(format!("Failed to parse UUID: {}", e)))?,
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
    let books = state.db.get_all::<Book>(DocumentTable::Books)?;

    let books_needing_metadata: Vec<(String, Book)> = books
        .into_iter()
        .map(|(id, mut book)| {
            book.has_metadata = false;
            (id, book)
        })
        .collect();

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
    let Some(book) = state.db.read::<Book>(DocumentTable::Books, &book_doc_id)? else {
        return Err(AppError::Internal(format!("Book not found: {book_doc_id}")));
    };

    let metadata_state = state.clone();

    tokio::spawn(async move {
        if let Err(err) = update_metadata(metadata_state, vec![(book_doc_id, book)]).await {
            error!(?err, "Metadata update failed");
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
pub mod test_helpers {
    use crate::api::init_resources::{Resources, patch_kobo_resources};
    use crate::database::MongoDatabase; // adjust to actual module path
    use crate::{AppState, app, config::AppConfig, database::document::DocumentDB};
    use axum::Router;
    use axum_test::TestServer;
    use dotenvy::dotenv;
    use std::env;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Connects to the MongoDB instance used for integration tests, reading
    /// `MONGODB_TEST_URI` from the environment. Panics with a clear message
    /// if the variable is unset or the connection fails, so a missing test
    /// dependency shows up immediately rather than as a confusing later error.
    pub async fn test_mongodb() -> MongoDatabase {
        // Load the .env file into the system environment
        dotenv().ok();

        let uri = env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set to run tests that require MongoDB");
        MongoDatabase::connect(&uri, "ebbooks_test")
            .await
            .expect("Failed to connect to MongoDB using MONGODB_TEST_URI")
    }

    /// Builds an `AppState` for tests, given a config and an in-memory
    /// `DocumentDB`. Centralizing this means callers only need to update one
    /// place (here) as `AppState`'s fields keep changing during the Mongo
    /// migration, instead of every test literal.
    pub async fn test_state(config: AppConfig, db: DocumentDB) -> AppState {
        let kobo_resources = Resources::default();
        let patched_resources = patch_kobo_resources(
            kobo_resources.clone(),
            &config.base_url,
            &config.ebbooks_auth_key,
            config.proxy_kobo_store,
        );

        AppState {
            config: Arc::new(config),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            mongodb: Arc::new(test_mongodb().await),
            hardcover_api_token: None,
            kobo_resources: Arc::new(Mutex::new(kobo_resources)),
            patched_resources: Arc::new(Mutex::new(patched_resources)),
        }
    }
    /// Helper utility to bootstrap a `TestServer` instance for integration testing.
    pub fn setup_test_app(state: AppState) -> TestServer {
        let router = Router::new().merge(app(state));
        TestServer::new(router)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::book::Book,
        scan::scanner::ScanResponse,
        test_helpers::{setup_test_app, test_state},
    };
    use axum::http::StatusCode;
    use std::path::PathBuf;
    use test_log::test;

    /// Tests the root endpoint response.
    #[test(tokio::test)]
    async fn test_root_handler() {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory().expect("Unable to open database");
        let state = test_state(config, db).await;
        let server = setup_test_app(state);
        let response = server.get("/").await;
        response.assert_status(StatusCode::OK);
    }

    /// Verifies that calling POST `/scan` triggers a background scan and returns a valid UUID.
    #[test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
    async fn test_scan_handler_triggers_scan() {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory().expect("Unable to open database");
        let state = test_state(config, db).await;

        let server = setup_test_app(state);

        let response = server.post("/scan").await;
        response.assert_status(StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(!body.scan_id.is_nil());
    }

    #[test(tokio::test)]
    async fn test_refresh_metadata_handler() {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory().expect("Unable to open database");
        let state = test_state(config, db).await;
        let server = setup_test_app(state);

        let response = server.post("/metadata/refresh").await;

        response.assert_status(StatusCode::NO_CONTENT);
    }

    #[test(tokio::test)]
    async fn test_refresh_single_book_metadata_handler() -> Result<(), Box<dyn std::error::Error>> {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory()?;

        let book = Book::from_path(PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        ));

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let state = test_state(config, db).await;
        let server = setup_test_app(state);

        let response = server.post(&format!("/metadata/refresh/{book_id}")).await;

        response.assert_status(StatusCode::NO_CONTENT);

        Ok(())
    }
}
