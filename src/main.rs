use crate::{
    api::{
        images::image_handler_with_quality,
        init_resources::{Resources, patch_kobo_resources},
        middleware::sync_auth_middleware,
    },
    config::AppConfig,
    database::MongoDatabase,
    error::AppError,
    library::book::Book,
    metadata::update_meta::update_metadata,
    scan::scanner::{ScanResponse, run_library_scan},
    scss::compile_scss,
};
use askama::Template;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{self, HeaderMap},
    middleware,
    response::Html,
    routing::{get, post},
};
use dotenvy::dotenv;
use mongodb::bson::{doc, oid::ObjectId};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{env, net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::services::ServeDir;
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub mod api;
pub mod auth;
pub(crate) mod config;
pub(crate) mod database;
pub(crate) mod directories;
pub mod error;
pub mod library;
pub mod logging;
pub(crate) mod metadata;
pub mod opds;
pub mod scan;
pub(crate) mod scss;

const DEFAULT_PER_PAGE: u64 = 25;
const MAX_PER_PAGE: u64 = 100;

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
    pub admin_key: Arc<str>,
}

impl AppState {
    /// Creates a new `AppState` instance with the given configuration.
    pub fn new(
        config: impl Into<Arc<AppConfig>>,
        mongodb: MongoDatabase,
        hardcover_api_token: Option<String>,
        admin_key: impl Into<Arc<str>>,
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
            admin_key: admin_key.into(),
        }
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    title: &'a str,
}

async fn index_handler() -> Result<Html<String>, AppError> {
    let html = IndexTemplate { title: "ebbooks" }.render()?;
    Ok(Html(html))
}

/// Constructs the main application `Router` and registers API routes with shared state
pub fn app(state: AppState) -> Router {
    // Heavy routes: scanning the library, generating images, and hitting the Hardcover API
    let gov = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(7) // refill: 1 request every 7s per IP
            .burst_size(10) // allow a short burst of 10 requests per IP
            .key_extractor(SmartIpKeyExtractor)
            .finish()
            .expect("valid governor config"),
    );

    // Prune stale per-IP entries so the limiter can't leak memory
    let limiter = gov.limiter().clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            limiter.retain_recent();
        }
    });

    let admin = Router::new()
        .route("/scan", post(scan_handler))
        .route("/library/info", get(library_info_handler))
        .route("/library/list", get(library_list_handler))
        .route("/metadata/refresh", post(refresh_metadata_handler))
        .route(
            "/metadata/refresh/{book_doc_id}",
            post(refresh_single_book_metadata_handler),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_admin,
        ))
        .layer(GovernorLayer::new(gov.clone()));

    let client_with_sync_auth_token = Router::new()
        .route(
            "/book/{token}/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}",
            get(image_handler_with_quality),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            sync_auth_middleware,
        ))
        .layer(GovernorLayer::new(gov));

    Router::new()
        .route("/", get(index_handler))
        .merge(admin)
        .merge(client_with_sync_auth_token)
        .merge(api::kobo_routes::kobo_routes(state.clone()))
        .nest_service("/static/styles", ServeDir::new("static/styles"))
        .with_state(state)
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
    debug!("Loaded config");

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

    let admin_key = env::var("ADMIN_KEY").map_err(|e| {
        error!("Could not find ADMIN_KEY: {e}");
        AppError::Internal(format!(
            "Missing required ADMIN_KEY environment variable: {e}"
        ))
    })?;
    if admin_key.len() < 32 {
        error!("ADMIN_KEY is too short");
        return Err(AppError::Internal(
            "ADMIN_KEY must be at least 32 characters".into(),
        ));
    }

    // Open DB
    // TODO Switch to being backed by MongoDB

    // TODO let scans = mongodb.scans.fetch_all()
    // // Debug all the scans stored in the database
    // let scans: Vec<(String, ScanDocument)> = db.get_all(DocumentTable::Scans)?;
    // debug!(?scans, count = scans.len(), "All stored scans in database");

    // Fetch and debug all stored books & scans from redb
    // let books: Vec<(String, Book)> = db.get_all(DocumentTable::Books)?;
    // debug!(?books, count = books.len(), "All stored books in database");

    // Compile SCSS styles
    debug!("Compiling scss styles");
    compile_scss()?;

    // Construct App state
    let state = AppState::new(config, mongodb, hardcover_api_token, admin_key);

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
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct LibraryInfo {
    pub library_path: String,
    pub total_books: u64,
    pub books_with_metadata: u64,
    pub books_needing_metadata: u64,
}

/// GET `/library/info`
///
/// Returns a quick summary of the library: how many books are indexed and
/// how many still need metadata from Hardcover.
pub async fn library_info_handler(
    State(state): State<AppState>,
) -> Result<Json<LibraryInfo>, AppError> {
    let (total_books, books_needing_metadata) = tokio::try_join!(
        state.mongodb.books.count_all(),
        state.mongodb.books.count_needing_metadata(),
    )?;

    Ok(Json(LibraryInfo {
        library_path: state.config.library_path.display().to_string(),
        total_books,
        books_with_metadata: total_books.saturating_sub(books_needing_metadata),
        books_needing_metadata,
    }))
}

/// POST `/scan` request handler
///
/// Generates a unique `scan_id`, launches an asynchronous scan task in the
/// background, and returns the generated UUID to the client immediately
pub async fn scan_handler(State(state): State<AppState>) -> Result<Json<ScanResponse>, AppError> {
    let scan_id = state.mongodb.scans.start().await?;

    info!(doc_id = ?scan_id, "Initialized scan");

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

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
}

fn default_page() -> u64 {
    1
}
fn default_per_page() -> u64 {
    DEFAULT_PER_PAGE
}

impl PaginationParams {
    /// Clamp user input to sane values.
    pub fn normalized(&self) -> (u64, u64) {
        let page = self.page.max(1);
        let per_page = self.per_page.clamp(1, MAX_PER_PAGE);
        (page, per_page)
    }

    pub fn skip(&self) -> u64 {
        let (page, per_page) = self.normalized();
        (page - 1) * per_page
    }
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub has_more: bool,
    /// URI of the next page, if there is one.
    pub next_page: Option<String>,
    pub page: u64,
    pub per_page: u64,
    pub total_items: u64,
    pub total_pages: u64,
}

impl<T> ListResponse<T> {
    pub fn new(
        data: Vec<T>,
        total_items: u64,
        params: &PaginationParams,
        base_path: &str, // e.g. "/library/list"
    ) -> Self {
        let (page, per_page) = params.normalized();
        let total_pages = total_items.div_ceil(per_page);
        let has_more = page < total_pages;
        let next_page =
            has_more.then(|| format!("{base_path}?page={}&per_page={per_page}", page + 1));

        Self {
            data,
            has_more,
            next_page,
            page,
            per_page,
            total_items,
            total_pages,
        }
    }
}

/// GET `/library/list?page=1&per_page=50`
pub async fn library_list_handler(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<Book>>, AppError> {
    let (_, per_page) = params.normalized();

    let (books, total) = state
        .mongodb
        .books
        .fetch_page(params.skip(), per_page as i64)
        .await?;

    Ok(Json(ListResponse::new(
        books,
        total,
        &params,
        "/library/list",
    )))
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
    pub const TEST_ADMIN_KEY: &str = "test-admin-key";
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
                admin_key: Arc::from(TEST_ADMIN_KEY),
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
        test_helpers::{AppTestContext, setup_test_app, write_minimal_epub},
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
        // TODO Use write_minimal_epub() here and assign test metadata instead
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");

        // Write a valid minimal EPUB file instead of zeroed bytes
        write_minimal_epub(&epub_path, "Test Book")?;
        let mut book = Book::from_path(epub_path);
        book.hardcover_slug = Some("hamlet".into());
        book.hardcover_id = Some(4083277);

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let server = setup_test_app(ctx.state.clone());

        let response = server.post(&format!("/metadata/refresh/{book_id}")).await;

        response.assert_status(StatusCode::NO_CONTENT);

        Ok(())
    }
}
