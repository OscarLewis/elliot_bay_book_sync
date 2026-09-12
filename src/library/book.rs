use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub path: Box<Path>,
    pub name: String,
    pub initial_format: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub size_kb: u64,
    pub modified_at: String,
    #[serde(default)]
    pub hardcover_id: Option<u64>,
    #[serde(default)]
    pub hardcover_slug: Option<String>,
    #[serde(default)]
    pub has_metadata: bool,
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
            hardcover_id: None,
            hardcover_slug: None,
            has_metadata: false,
            modified_at: String::new(),
        }
    }

    pub fn from_path(path: impl Into<Box<Path>>) -> Self {
        let path = path.into();

        let name = path
            .file_stem()
            .and_then(|stem| {
                if Path::new(stem)
                    .extension()
                    .is_some_and(|ext| ext == "kepub")
                {
                    Path::new(stem).file_stem()
                } else {
                    Some(stem)
                }
            })
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();

        let initial_format = {
            // Check if it's a 'Book.kepub.epub' file first
            if path
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|ext| ext == "kepub")
                && path.extension().is_some_and(|ext| ext == "epub")
            {
                Some("kepub".to_owned())
            } else {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_owned)
            }
        };

        // Query file metadata to obtain file size in kilobytes
        let size_kb = std::fs::metadata(&path)
            .map(|meta| meta.len() / 1024)
            .unwrap_or(0);

        let modified_at = std::fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|time| DateTime::<Utc>::from(time).to_rfc3339())
            .unwrap_or_default();

        // TODO Add title and author from epub extraction

        Self {
            path,
            name,
            initial_format,
            size_kb,
            modified_at,
            ..Default::default()
        }
    }
}
