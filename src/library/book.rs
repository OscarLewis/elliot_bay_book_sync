use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Book {
    pub path: Box<Path>,
    pub name: String,
    pub id: Uuid,
    pub initial_format: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
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
            id: Uuid::new_v4(),
            initial_format: None,
            author: None,
            title: None,
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
            .map(str::to_owned);

        Self {
            path,
            name,
            id: Uuid::new_v4(),
            initial_format,
            ..Default::default()
        }
    }
}
