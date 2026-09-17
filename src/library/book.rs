use chrono::{DateTime, Utc};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub path: Box<Path>,
    // MongoDB ID value
    // TODO FIX this and get ID working for mongo db documents
    // #[serde(rename = "_id", default)]
    // pub id: String,
    pub name: String,
    pub initial_format: Option<String>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub series_name: Option<String>,
    pub series_position: Option<f64>,
    #[serde(default)]
    pub description: Option<String>,
    pub size_kb: u64,
    // TODO with switch to MongoDB turn this into an actual date tiem
    pub modified_at: String,
    #[serde(default)]
    pub hardcover_id: Option<u64>,
    #[serde(default)]
    pub hardcover_slug: Option<String>,
    #[serde(default)]
    pub hardcover_img_id: Option<u64>,
    #[serde(default)]
    pub hardcover_img_url: Option<String>,
    #[serde(default)]
    pub hardcover_series_id: Option<u64>,
    #[serde(default)]
    pub has_metadata: bool,
    #[serde(default)]
    pub has_image: bool,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub reading_state: Option<ReadingStateDocument>,
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
            // id: String::new(),
            initial_format: None,
            author: None,
            title: None,
            series_name: None,
            series_position: None,
            description: None,
            size_kb: 0,
            hardcover_id: None,
            hardcover_slug: None,
            hardcover_img_id: None,
            hardcover_img_url: None,
            hardcover_series_id: None,
            has_metadata: false,
            has_image: false,
            image_path: None,
            modified_at: String::new(),
            reading_state: None,
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

        Self {
            path,
            name,
            initial_format,
            size_kb,
            modified_at,
            ..Default::default()
        }
    }

    pub fn image(&self) -> Result<Option<DynamicImage>, AppError> {
        let Some(image_path) = &self.image_path else {
            return Ok(None);
        };

        let image = image::open(image_path)?;

        Ok(Some(image))
    }
}

/** # Reading State tracking
 * The Kobo ReadingState API keeps track of 4 timestamped entities: ReadingState, StatusInfo, Statistics, CurrentBookmark
*/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStateDocument {
    pub book_id: String,
    pub entitlement_id: String,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub priority_timestamp: DateTime<Utc>,
    pub status_info: StatusInfoDocument,
    pub statistics: Option<StatisticsDocument>,
    pub current_bookmark: Option<CurrentBookmarkDocument>,
}

/// Read Status enum, 1 means finished, 2 means in progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    Unread,
    Finished,
    InProgress,
}

impl ReadStatus {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => ReadStatus::Finished,
            2 => ReadStatus::InProgress,
            _ => ReadStatus::Unread,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            ReadStatus::Unread => 0,
            ReadStatus::Finished => 1,
            ReadStatus::InProgress => 2,
        }
    }
}

impl fmt::Display for ReadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = match self {
            ReadStatus::Unread => "Unread",
            ReadStatus::InProgress => "Reading",
            ReadStatus::Finished => "Finished",
        };
        write!(f, "{}", status_str)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfoDocument {
    pub status: ReadStatus,
    pub last_modified: DateTime<Utc>,
    pub last_time_started_reading: Option<DateTime<Utc>>,
    pub times_started_reading: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsDocument {
    pub last_modified: DateTime<Utc>,
    pub remaining_time_minutes: Option<i32>,
    pub spent_reading_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentBookmarkDocument {
    pub last_modified: DateTime<Utc>,
    pub location_source: Option<String>,
    pub location_type: Option<String>,
    pub location_value: Option<String>,
    pub progress_percent: Option<f64>,
    pub content_source_progress_percent: Option<f64>,
}
