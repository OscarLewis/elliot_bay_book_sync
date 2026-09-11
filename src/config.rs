use std::{path::Path, sync::Arc};

const LIBRARY_PATH: &str = "test ebooks";
const PROXY_KOBO_STORE: bool = true;
const DB_PATH: &str = "sync_db.redb";

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub library_path: Arc<Path>,
    pub proxy_kobo_store: bool,
    pub database_path: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: Arc::from(Path::new(LIBRARY_PATH)),
            proxy_kobo_store: PROXY_KOBO_STORE,
            database_path: DB_PATH.to_string(),
        }
    }
}

impl AppConfig {
    pub fn new(
        library_path: impl AsRef<Path>,
        proxy_kobo_store: bool,
        database_path: impl Into<String>,
    ) -> Self {
        Self {
            library_path: Arc::from(library_path.as_ref()),
            proxy_kobo_store,
            database_path: database_path.into(),
        }
    }
}
