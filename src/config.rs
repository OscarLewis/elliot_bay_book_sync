use std::{path::Path, sync::Arc};

const LIBRARY_PATH: &str = "test ebooks";
const PROXY_KOBO_STORE: bool = true;
const DB_PATH: &str = "sync_db.redb";
const IMG_PATH: &str = "static/images";
const TEST_AUTH_KEY: &str = "test-key-123";
const BASE_URL: &str = "http://localhost:3000";

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub library_path: Arc<Path>,
    pub proxy_kobo_store: bool,
    pub database_path: String,
    pub image_path: String,
    pub ebbooks_auth_key: String,
    pub base_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: Arc::from(Path::new(LIBRARY_PATH)),
            proxy_kobo_store: PROXY_KOBO_STORE,
            database_path: DB_PATH.to_string(),
            image_path: IMG_PATH.to_string(),
            ebbooks_auth_key: TEST_AUTH_KEY.to_string(),
            base_url: BASE_URL.to_string(),
        }
    }
}

impl AppConfig {
    pub fn new(
        library_path: Option<impl AsRef<Path>>,
        proxy_kobo_store: Option<bool>,
        database_path: Option<impl Into<String>>,
        image_path: Option<impl Into<String>>,
        ebbooks_auth_key: Option<impl Into<String>>,
        base_url: Option<impl Into<String>>,
    ) -> Self {
        let default = Self::default();

        Self {
            library_path: library_path
                .map(|p| Arc::from(p.as_ref()))
                .unwrap_or(default.library_path),
            proxy_kobo_store: proxy_kobo_store.unwrap_or(default.proxy_kobo_store),
            database_path: database_path
                .map(Into::into)
                .unwrap_or(default.database_path),
            image_path: image_path.map(Into::into).unwrap_or(default.image_path),
            ebbooks_auth_key: ebbooks_auth_key
                .map(Into::into)
                .unwrap_or(default.ebbooks_auth_key),
            base_url: base_url.map(Into::into).unwrap_or(default.base_url),
        }
    }
}
