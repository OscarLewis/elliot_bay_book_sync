use crate::{
    config::AppConfig,
    library::book::{
        Book, CurrentBookmarkDocument, ReadingStateDocument, StatisticsDocument, StatusInfoDocument,
    },
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ActivePeriod {
    pub from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Price {
    pub currency_code: String,
    pub total_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]

pub struct LovePrice {
    pub total_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DownloadUrl {
    pub format: String,
    pub size: i64,
    pub url: String,
    pub platform: String,
    pub drm_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]

pub struct Publisher {
    pub imprint: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]

pub struct ContributorRole {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Series {
    pub name: String,
    pub number: i32,
    pub number_float: f64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]

pub struct ReadingState {
    pub entitlement_id: String,
    pub created: String,
    pub last_modified: String,
    pub priority_timestamp: String,
    pub status_info: StatusInfo,
    pub statistics: Statistics,
    pub current_bookmark: CurrentBookmark,
}

impl ReadingState {
    pub fn from_document(doc: &ReadingStateDocument) -> Self {
        Self {
            entitlement_id: doc.entitlement_id.clone(),
            created: doc.created.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            last_modified: doc.last_modified.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            priority_timestamp: doc
                .priority_timestamp
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
            status_info: StatusInfo::from_document(doc.status_info.clone()),
            statistics: doc
                .statistics
                .clone()
                .map(Statistics::from_document)
                .unwrap_or_default(),
            current_bookmark: doc
                .current_bookmark
                .clone()
                .map(CurrentBookmark::from_document)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StatusInfo {
    pub last_modified: String,
    pub status: String,
    pub times_started_reading: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_time_started_reading: Option<String>,
}

impl StatusInfo {
    pub fn from_document(doc: StatusInfoDocument) -> Self {
        Self {
            status: doc.status.to_string(),
            last_modified: doc.last_modified.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            last_time_started_reading: doc
                .last_time_started_reading
                .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            times_started_reading: doc.times_started_reading,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Statistics {
    pub last_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spent_reading_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_time_minutes: Option<i32>,
}

impl Statistics {
    pub fn from_document(doc: StatisticsDocument) -> Self {
        Self {
            last_modified: doc.last_modified.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            remaining_time_minutes: doc.remaining_time_minutes,
            spent_reading_minutes: doc.spent_reading_minutes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CurrentBookmark {
    pub last_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_source_progress_percent: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

impl CurrentBookmark {
    pub fn from_document(doc: CurrentBookmarkDocument) -> Self {
        // Construct Location only if all required fields are present
        let location = match (doc.location_value, doc.location_type, doc.location_source) {
            (Some(value), Some(location_type), Some(source)) => Some(Location {
                value,
                location_type,
                source,
            }),
            _ => None,
        };

        Self {
            last_modified: doc.last_modified.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            progress_percent: doc.progress_percent.map(|p| p as i32),
            content_source_progress_percent: doc.content_source_progress_percent.map(|p| p as i32),
            location,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Location {
    pub value: String,
    #[serde(rename = "Type")]
    pub location_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Tag {
    pub created: String,
    pub id: String,
    pub items: Vec<TagItem>,
    pub last_modified: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub tag_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TagItem {
    pub revision_id: String,
    #[serde(rename = "Type")]
    pub item_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct BookMetadata {
    pub categories: Vec<String>,
    pub cover_image_id: String,
    pub cross_revision_id: String,
    pub current_display_price: Price,
    pub current_love_display_price: LovePrice,
    pub description: Option<String>,
    pub download_urls: Vec<DownloadUrl>,
    pub entitlement_id: String,
    pub external_ids: Vec<String>,
    pub genre: String,
    pub is_eligible_for_kobo_love: bool,
    pub is_internet_archive: bool,
    pub is_pre_order: bool,
    pub is_social_enabled: bool,
    pub language: String,
    pub phonetic_pronunciations: std::collections::HashMap<String, String>,
    pub publication_date: String,
    pub publisher: Publisher,
    pub revision_id: String,
    pub title: String,
    pub work_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributors: Option<Vec<Contributor>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contributor_roles: Option<Vec<ContributorRole>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<Series>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct BookEntitlement {
    pub accessibility: String,
    pub active_period: ActivePeriod,
    pub created: String,
    pub cross_revision_id: String,
    pub id: String,
    pub is_removed: bool,
    pub is_hidden_from_archive: bool,
    pub is_locked: bool,
    pub last_modified: String,
    pub origin_category: String,
    pub revision_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingStateChange {
    pub reading_state: ReadingState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedTag {
    pub tag: Tag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Contributor {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SyncResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_entitlement: Option<Entitlement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_entitlement: Option<Entitlement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_reading_state: Option<ReadingStateChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_tag: Option<DeletedTag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_tag: Option<Tag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_tag: Option<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Entitlement {
    pub book_entitlement: BookEntitlement,
    pub book_metadata: BookMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_state: Option<ReadingState>,
}

impl Entitlement {
    pub fn from_book_tuple(
        (book_id, book): (&str, &Book),
        app_config: Arc<AppConfig>,
        accessibility: Option<String>,
        active_from: Option<String>,
        reading_state: Option<ReadingState>,
        archieved: bool,
    ) -> Self {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let last_modified = book.modified_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let book_entitlement = BookEntitlement {
            id: book_id.to_string(),
            accessibility: accessibility.unwrap_or_else(|| "Full".to_string()),
            active_period: ActivePeriod {
                from: active_from.unwrap_or_else(|| now.clone()),
            },
            created: now,
            cross_revision_id: book_id.to_string(),
            is_removed: archieved,
            is_hidden_from_archive: false,
            is_locked: false,
            last_modified: last_modified,
            origin_category: "Imported".to_string(),
            revision_id: book_id.to_string(),
            status: "Active".to_string(),
        };

        let book_metadata = BookMetadata::from_book_tuple((&book_id, book), app_config);

        Entitlement {
            book_entitlement,
            book_metadata,
            reading_state,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KoboFormat {
    Kepub,
    Epub,
}

impl KoboFormat {
    pub fn download_format(self) -> &'static str {
        match self {
            Self::Kepub => "kepub",
            Self::Epub => "epub",
        }
    }

    pub fn published_format(self) -> &'static str {
        match self {
            Self::Kepub => "KEPUB",
            Self::Epub => "EPUB3",
        }
    }
}

pub fn get_download_url_for_book_id(
    base_url: &str,
    auth_token: &str,
    format: KoboFormat,
    book_id: &str,
) -> String {
    format!(
        "{}kobo/{}/download/{}/{}",
        base_url,
        auth_token,
        book_id,
        format.download_format(),
    )
}

// TODO Change this to take references
impl BookMetadata {
    pub fn from_book_tuple((book_id, book): (&str, &Book), app_config: Arc<AppConfig>) -> Self {
        // Parse UTC timestamp or fallback to `now`
        let pub_date = book.modified_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        // Derive format & download details
        let format = match book.initial_format.as_deref() {
            Some("kepub") => KoboFormat::Kepub,
            _ => KoboFormat::Epub,
        };
        // TODO this should return a fucking enum

        // Size in bytes (size_kb * 1024)
        let download_urls = vec![DownloadUrl {
            format: format.published_format().to_string(),
            size: (book.size_kb * 1024) as i64,
            url: get_download_url_for_book_id(
                &app_config.base_url,
                &app_config.ebbooks_auth_key,
                format,
                book_id,
            ),
            platform: "Generic".to_string(),
            drm_type: "None".to_string(),
        }];

        debug!(
            ?download_urls,
            book_id = book_id,
            "Download urls generated for Book"
        );

        // Contributors list
        let contributors = book
            .author
            .as_ref()
            .map(|author| {
                vec![Contributor {
                    name: author.clone(),
                }]
            })
            .unwrap_or_default();

        // Series payload calculation
        let series = book.series_name.as_ref().map(|name| {
            let pos = book.series_position.unwrap_or(1.0);
            let series_uuid = Uuid::new_v3(&Uuid::NAMESPACE_DNS, name.as_bytes()).to_string();

            Series {
                id: series_uuid,
                name: name.clone(),
                number: pos as i32,
                number_float: pos,
            }
        });

        let title = book.title.clone().unwrap_or_else(|| book.name.clone());

        // TODO Clean this up
        let contributor_roles = book.author.as_ref().map(|author| {
            vec![ContributorRole {
                name: author.clone(),
            }]
        });

        Self {
            categories: vec!["00000000-0000-0000-0000-000000000001".to_string()],
            contributors: Some(contributors),
            // contributor_roles,
            contributor_roles: Some(vec![]),
            cover_image_id: book_id.to_string(),
            cross_revision_id: book_id.to_string(),
            current_display_price: Price {
                currency_code: "USD".to_string(),
                total_amount: 0.0,
            },
            current_love_display_price: LovePrice { total_amount: 0.0 },
            description: book.description.clone(),
            download_urls,
            entitlement_id: book_id.to_string(),
            external_ids: vec![],
            genre: "00000000-0000-0000-0000-000000000001".to_string(),
            is_eligible_for_kobo_love: false,
            is_internet_archive: false,
            is_pre_order: false,
            is_social_enabled: true,
            language: "en".to_string(),
            publication_date: pub_date,
            publisher: Publisher {
                imprint: String::new(),
                name: String::new(),
            },
            revision_id: book_id.to_string(),
            title,
            work_id: book_id.to_string(),
            series,
            phonetic_pronunciations: HashMap::new(),
        }
    }
}
