use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Book {
    pub path: Box<Path>,
    pub name: String,
    pub id: Uuid,
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

        Self {
            path,
            name,
            id: Uuid::new_v4(),
            ..Default::default()
        }
    }
}
