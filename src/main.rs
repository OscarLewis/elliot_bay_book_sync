use crate::{
    config::AppConfig,
    database::document::{DocumentDB, DocumentTable},
    error::AppError,
    library::book::Book,
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
use std::env;
use std::{sync::Arc, time::Duration};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub mod api;
pub(crate) mod config;
pub(crate) mod database;
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
}

impl AppState {
    /// Creates a new `AppState` instance with the given configuration.
    pub fn new(config: impl Into<Arc<AppConfig>>, db: DocumentDB) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Kobo Touch/4.38.21908 (Linux 2.6.35.3; U; en-US)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        AppState {
            db: Arc::new(db),
            config: config.into(),
            req_client: client,
        }
    }
}

/// Constructs the main application `Router` and registers API routes with shared state
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "Ebook Sync Server" }))
        .route("/scan", post(scan_handler))
        .merge(api::kobo_routes::kobo_routes())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(logging::log_with_body))
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // Initialize logging subscriber using environment filter (defaults to debug level for kobo_sync_rs)
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("kobo_sync_rs=debug,tower_http=info")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load default config
    let config = AppConfig::default();

    // Load the .env file into the system environment
    dotenv().ok();
    // Panic if there is no Hardcover token for metadata
    match env::var("HARDCOVER_TOKEN") {
        Ok(val) => debug!("Hardcover Token: {val}"),
        Err(e) => error!("Could not find HARDCOVER_TOKEN: {e}"),
    }

    // Fetch and log all stored books & scans from redb
    let db = DocumentDB::open(&config.database_path)?;
    let books: Vec<(String, Book)> = db.get_all(DocumentTable::Books)?;
    debug!(?books, count = books.len(), "All stored books in database");

    // Filter through set of all books for those with has_metadata = False
    let books_needing_metadata: Vec<(String, Book)> = books
        .into_iter()
        .filter(|(_, book)| !book.has_metadata)
        .collect();

    debug!(
        count = books_needing_metadata.len(),
        "Files needing metadata"
    );

    if !books_needing_metadata.is_empty() {
        for (id, book) in books_needing_metadata {
            // TODO lets update some damn metadata
            // id -> document ID for update
            // book -> metadata work
        }
        // Actually just pass the whole damn Vec of tuples to the helper function
    }

    // Debug all the scans stored in the database
    let scans: Vec<(String, ScanDocument)> = db.get_all(DocumentTable::Scans)?;
    debug!(?scans, count = scans.len(), "All stored scans in database");

    // Construct App state
    let state = AppState::new(config, db);

    // Bind state to app
    let app = app(state);

    // Bind server to local port 3000
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
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

    // Clone for the spawned task
    let doc_id_for_task = record_doc_id.clone();

    // Spawn long-running library scanning task asynchronously so handler returns immediately
    tokio::spawn(run_library_scan(
        state.db.clone(),
        doc_id_for_task,
        state.config.library_path.clone(),
    ));

    Ok(Json(ScanResponse {
        scan_id: Uuid::parse_str(&record_doc_id)
            .map_err(|e| AppError::Internal(format!("Failed to parse UUID: {}", e)))?,
    }))
}

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

#[cfg(test)]
pub mod test_helpers {
    use crate::{AppState, app};
    use axum::Router;
    use axum_test::TestServer;

    /// Helper utility to bootstrap a `TestServer` instance for integration testing.
    pub fn setup_test_app(state: AppState) -> TestServer {
        let router = Router::new().merge(app(state));
        TestServer::new(router)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState, config::AppConfig, database::document::DocumentDB, scan::scanner::ScanResponse,
        test_helpers::setup_test_app,
    };
    use axum::http::StatusCode;
    use test_log::test;

    /// Tests the root endpoint response.
    #[test(tokio::test)]
    async fn test_root_handler() {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory().expect("Unable to open database");
        let state = AppState::new(config, db);
        let server = setup_test_app(state);
        let response = server.get("/").await;
        response.assert_status(StatusCode::OK);
    }

    /// Verifies that calling POST `/scan` triggers a background scan and returns a valid UUID.
    #[test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
    async fn test_scan_handler_triggers_scan() {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory().expect("Unable to open database");
        let state = AppState::new(config, db);

        let server = setup_test_app(state);

        let response = server.post("/scan").await;
        response.assert_status(StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(!body.scan_id.is_nil());
    }
}
