use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub path: Box<Path>,
    pub name: String,
    pub initial_format: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub size_kb: u64,
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}

impl Book {
    pub fn new() -> Self {
        Self {
            path: Path::new("").into(),
            name: String::new(),
            initial_format: None,
            author: None,
            title: None,
            size_kb: 0,
        }
    }

    pub fn from_path(path: impl Into<Box<Path>>) -> Self {
        let path = path.into();

        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();

        let initial_format = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_owned); //TODO ".kepub.epub"

        // Query file metadata to obtain file size in kilobytes
        let size_kb = std::fs::metadata(&path)
            .map(|meta| meta.len() / 1024)
            .unwrap_or(0);

        Self {
            path,
            name,
            initial_format,
            size_kb,
            ..Default::default()
        }
    }
}
