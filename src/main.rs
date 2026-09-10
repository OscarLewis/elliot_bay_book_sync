use crate::{
    config::AppConfig,
    scan::{scanner::ScanResponse, status::ScanStatus},
};
use axum::{
    Json, Router,
    extract::State,
    http::{self, HeaderMap},
    middleware,
    routing::{get, post},
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{debug, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
pub mod api;
pub(crate) mod config;
pub mod logging;
pub mod scan;

/// Holds shared application state accessible across request handlers.
#[derive(Clone)]
pub struct AppState {
    /// Global handle for monitoring and dispatching scan status updates.
    pub scan_status: ScanStatus,
    /// Shared application configuration settings.
    pub config: Arc<AppConfig>,
}

impl AppState {
    /// Creates a new `AppState` instance with the given configuration.
    pub fn new(config: impl Into<Arc<AppConfig>>) -> Self {
        AppState {
            scan_status: ScanStatus::new(),
            config: config.into(),
        }
    }
}

/// Constructs the main application `Router` and registers API routes with shared state.
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
async fn main() {
    // Initialize logging subscriber using environment filter (defaults to debug level for kobo_sync_rs)
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("kobo_sync_rs=debug,tower_http=debug")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::default();
    let state = AppState::new(config);

    // Subscribe to scan status updates before starting the server
    let mut status_rx = state.scan_status.subscribe();

    // Background task to process and log broadcast scan status events
    tokio::spawn(async move {
        while let Ok(message) = status_rx.recv().await {
            debug!(?message, "Received scan status");
        }
    });

    let app = app(state);

    // Bind server to local port 3000
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    info!("listening on {}", listener.local_addr().unwrap());

    // Start serving HTTP requests
    axum::serve(listener, app).await.unwrap();
}

/// POST `/scan` request handler.
///
/// Generates a unique `scan_id`, launches an asynchronous scan task in the
/// background, and returns the generated UUID to the client immediately.
pub async fn scan_handler(State(state): State<AppState>) -> Json<ScanResponse> {
    let scan_id = Uuid::new_v4();
    let status = state.scan_status.clone();

    // Spawn long-running library scanning task asynchronously so handler returns immediately
    tokio::spawn(async move {
        scan::scanner::scan_library(status, scan_id, &state.config.library_path).await;
    });

    Json(ScanResponse { scan_id })
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
        AppState, config::AppConfig, scan::scanner::ScanResponse, test_helpers::setup_test_app,
    };
    use axum::http::StatusCode;
    use test_log::test;

    /// Tests the root endpoint response.
    #[test(tokio::test)]
    async fn test_root_handler() {
        let config = AppConfig::default();
        let state = AppState::new(config);
        let server = setup_test_app(state);
        let response = server.get("/").await;
        response.assert_status(StatusCode::OK);
    }

    /// Verifies that calling POST `/scan` triggers a background scan and returns a valid UUID.
    #[test(tokio::test(flavor = "multi_thread", worker_threads = 2))]
    async fn test_scan_handler_triggers_scan() {
        let config = AppConfig::default();
        let state = AppState::new(config);

        let server = setup_test_app(state);

        let response = server.post("/scan").await;
        response.assert_status(StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(!body.scan_id.is_nil());
    }
}
