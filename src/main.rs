use crate::scan::{scanner::ScanResponse, status::ScanStatus};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use tracing::{debug, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

pub mod scan;

/// Holds shared application state accessible across request handlers.
#[derive(Clone)]
pub(crate) struct AppState {
    /// Global handle for monitoring and dispatching scan status updates.
    pub(crate) scan_status: ScanStatus,
}

impl AppState {
    /// Creates a new `AppState` instance initialized with default channels.
    pub(crate) fn new() -> Self {
        AppState {
            scan_status: ScanStatus::new(),
        }
    }
}

/// Constructs the main application `Router` and registers API routes with shared state.
pub(crate) fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!" }))
        .route("/scan", post(scan_handler))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    // Initialize logging subscriber using environment filter (defaults to debug level for kobo_sync_rs)
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("kobo_sync_rs=debug")),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::new();

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
pub(crate) async fn scan_handler(State(state): State<AppState>) -> Json<ScanResponse> {
    let scan_id = Uuid::new_v4();
    let status = state.scan_status.clone();

    // Spawn long-running library scanning task asynchronously so handler returns immediately
    tokio::spawn(async move {
        scan::scanner::scan_library(status, scan_id).await;
    });

    Json(ScanResponse { scan_id })
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use crate::{AppState, app};
    use axum_test::TestServer;

    /// Helper utility to bootstrap a `TestServer` instance for integration testing.
    pub(crate) fn setup_test_app(state: AppState) -> TestServer {
        TestServer::new(app(state))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        scan::{
            scanner::ScanResponse,
            status::ScanStatus,
        },
        test_helpers::setup_test_app,
    };
    use axum::http::StatusCode;

    /// Tests the root endpoint response.
    #[tokio::test]
    async fn test_root_handler() {
        let state = AppState::new();
        let server = setup_test_app(state);
        let response = server.get("/").await;
        response.assert_status(StatusCode::OK);
    }

    /// Verifies that calling POST `/scan` triggers a background scan and returns a valid UUID.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_scan_handler_triggers_scan() {
        let state = AppState::new();

        let server = setup_test_app(state);

        let response = server.post("/scan").await;
        response.assert_status(StatusCode::OK);

        let body: ScanResponse = response.json();
        assert!(!body.scan_id.is_nil());
    }
}