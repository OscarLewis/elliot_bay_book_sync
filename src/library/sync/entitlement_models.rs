use crate::{config::AppConfig, library::book::Book};
use chrono::{DateTime, Utc};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StatusInfo {
    pub last_modified: String,
    pub status: String,
    pub times_started_reading: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_time_started_reading: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Statistics {
    pub last_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spent_reading_minutes: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_time_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let last_modified = DateTime::parse_from_rfc3339(&book.modified_at)
            .map(|dt| {
                dt.with_timezone(&Utc)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
            })
            .unwrap_or_else(|_| book.modified_at.clone());

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
        let pub_date = DateTime::parse_from_rfc3339(&book.modified_at)
            .map(|dt| {
                dt.with_timezone(&Utc)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
            })
            .unwrap_or_else(|_| Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
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
        let _contributor_roles = book.author.as_ref().map(|author| {
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

// TODO: Implement From (String, Book) for BookMetadata (part of an Entitlement)
// impl From<(String, Book)> for BookMetadata {
//     fn from((book_id, book): (String, Book)) -> Self {
//         // TODO: Map fields from `book` as needed
//         Self {
//             // title: book.title, // example mapping

//             // Fills all remaining fields with their type's default value
//             ..Default::default()
//         }
//     }
// }
/*
def get_metadata(book):
    download_urls = []

    kepub_data = next((d for d in book.data if d.format == 'KEPUB'), None)
    epub_data  = next((d for d in book.data if d.format == 'EPUB'),  None)

    if kepub_data:
        book_data, dl_format, published_format = kepub_data, 'kepub', 'KEPUB'
    elif epub_data and config.config_kepubifypath:
        book_data, dl_format, published_format = epub_data, 'kepub', 'KEPUB'
    elif epub_data:
        book_data, dl_format, published_format = epub_data, 'epub', 'EPUB3'
    else:
        book_data = None

    if book_data:
        try:
            if get_epub_layout(book, book_data) == 'pre-paginated':
                published_format = 'EPUB3FL'
        except (zipfile.BadZipfile, FileNotFoundError) as e:
            log.error(e)
        download_urls.append({
            "Format": published_format,
            "Size": book_data.uncompressed_size,
            "Url": get_download_url_for_book(book.id, dl_format),
            "Platform": "Generic",
            "DrmType": "None",
        })

    book_uuid = book.uuid
    cover_image_id = _get_cover_image_id(book)
    if cover_image_id != str(book_uuid):
        log.debug("Kobo Sync: cache-busting cover id for book %s: %s", book.id, cover_image_id)
    metadata = {
        "Categories": ["00000000-0000-0000-0000-000000000001", ],
        # "Contributors": get_author(book),
        "CoverImageId": cover_image_id,
        "CrossRevisionId": book_uuid,
        "CurrentDisplayPrice": {"CurrencyCode": "USD", "TotalAmount": 0},
        "CurrentLoveDisplayPrice": {"TotalAmount": 0},
        "Description": get_description(book),
        "DownloadUrls": download_urls,
        "EntitlementId": book_uuid,
        "ExternalIds": [],
        "Genre": "00000000-0000-0000-0000-000000000001",
        "IsEligibleForKoboLove": False,
        "IsInternetArchive": False,
        "IsPreOrder": False,
        "IsSocialEnabled": True,
        "Language": get_language(book),
        "PhoneticPronunciations": {},
        "PublicationDate": convert_to_kobo_timestamp_string(book.pubdate),
        "Publisher": {"Imprint": "", "Name": get_publisher(book), },
        "RevisionId": book_uuid,
        "Title": book.title,
        "WorkId": book_uuid,
    }
    metadata.update(get_author(book))

    series_name = get_series(book)
    if series_name:
        name = series_name
        try:
            metadata["Series"] = {
                "Name": series_name,
                "Number": get_seriesindex(book),        # ToDo Check int() ?
                "NumberFloat": float(get_seriesindex(book)),
                # Get a deterministic id based on the series name.
                "Id": str(uuid.uuid3(uuid.NAMESPACE_DNS, name)),
            }
        except Exception as e:
            print(e)
    return metadata

    def create_book_entitlement(book, archived):
    book_uuid = str(book.uuid)
    return {
        "Accessibility": "Full",
        "ActivePeriod": {"From": convert_to_kobo_timestamp_string(datetime.now(timezone.utc))},
        "Created": convert_to_kobo_timestamp_string(book.timestamp),
        "CrossRevisionId": book_uuid,
        "Id": book_uuid,
        "IsRemoved": archived,
        "IsHiddenFromArchive": False,
        "IsLocked": False,
        "LastModified": convert_to_kobo_timestamp_string(book.last_modified),
        "OriginCategory": "Imported",
        "RevisionId": book_uuid,
        "Status": "Active",
    }




*/
