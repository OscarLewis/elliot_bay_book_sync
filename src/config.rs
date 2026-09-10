// config.rs
use std::{path::Path, sync::Arc};

const LIBRARY_PATH: &str = "/homes/oscar/Documents/Projects/kobo_sync_rs/test ebooks";
const PROXY_KOBO_STORE: bool = true;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub library_path: Arc<Path>,
    pub proxy_kobo_store: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: Arc::from(Path::new(LIBRARY_PATH)),
            proxy_kobo_store: PROXY_KOBO_STORE,
        }
    }
}

impl AppConfig {
    pub fn new(library_path: impl AsRef<Path>, proxy_kobo_store: bool) -> Self {
        Self {
            library_path: Arc::from(library_path.as_ref()),
            proxy_kobo_store,
        }
    }
}
