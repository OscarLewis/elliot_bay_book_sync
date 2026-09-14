use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/*
The Kobo ReadingState API keeps track of 4 timestamped entities: ReadingState, StatusInfo, Statistics, CurrentBookmark

Most of this file is just dedicated to tracking these in ReadingStateDocument

// TODO Implement reading state so it saves the kobo reading state to the database
// Just be passive observers for now, record when it comes in and then put whatever has JUST been
// recorded back in the Sync token.

/// KoboAnnotationSync tracks a different lifecycle (one row per annotation,
/// which may eventually get synced independently to Hardcover),
/// so it doesn't belong nested inside ReadingStateDocument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationSyncDocument {
    pub user_id: String,
    pub annotation_id: String,
    pub book_id: String,
    pub synced_to_hardcover: bool,
    pub hardcover_journal_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub last_synced: DateTime<Utc>,
    pub highlighted_text: Option<String>,
    pub highlight_color: Option<String>,
    pub note_text: Option<String>,
}
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

/// Embedded sub-document for ReadBook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfoDocument {
    pub status: ReadStatus,
    pub last_modified: DateTime<Utc>,
    pub last_time_started_reading: Option<DateTime<Utc>>,
    pub times_started_reading: i32,
}

/// Embedded sub-document for KoboStatistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsDocument {
    pub last_modified: DateTime<Utc>,
    pub remaining_time_minutes: Option<i32>,
    pub spent_reading_minutes: Option<i32>,
}

/// Embedded sub-document for KoboBookmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentBookmarkDocument {
    pub last_modified: DateTime<Utc>,
    pub location_source: Option<String>,
    pub location_type: Option<String>,
    pub location_value: Option<String>,
    pub progress_percent: Option<f64>,
    pub content_source_progress_percent: Option<f64>,
}

#[cfg(test)]
mod tests {
    use crate::database::document::{DocumentDB, DocumentTable};
    use crate::error::AppError;
    use crate::library::sync::reading_state_document::{
        CurrentBookmarkDocument, ReadStatus, ReadingStateDocument, StatisticsDocument,
        StatusInfoDocument,
    };
    use chrono::Utc;
    use test_log::test;

    fn sample_doc(book_id: &str) -> ReadingStateDocument {
        ReadingStateDocument {
            book_id: book_id.to_string(),
            entitlement_id: "ent-1234".to_string(),
            created: Utc::now(),
            last_modified: Utc::now(),
            priority_timestamp: Utc::now(),
            status_info: StatusInfoDocument {
                status: ReadStatus::InProgress,
                last_modified: Utc::now(),
                last_time_started_reading: Some(Utc::now()),
                times_started_reading: 1,
            },
            statistics: Some(StatisticsDocument {
                last_modified: Utc::now(),
                remaining_time_minutes: Some(42),
                spent_reading_minutes: Some(58),
            }),
            current_bookmark: Some(CurrentBookmarkDocument {
                last_modified: Utc::now(),
                location_source: Some("epub".to_string()),
                location_type: Some("KoboSpan".to_string()),
                location_value: Some("/6/4[chap01]!/4/2/1:0".to_string()),
                progress_percent: Some(37.5),
                content_source_progress_percent: Some(37.5),
            }),
        }
    }

    #[test(test)]
    fn create_and_read_round_trips() -> Result<(), AppError> {
        let db = DocumentDB::open_in_memory()?;
        let doc = sample_doc("book-1");

        let id = db.create(DocumentTable::ReadingStates, &doc, None, None)?;

        let fetched: ReadingStateDocument = db
            .read(DocumentTable::ReadingStates, &id)?
            .expect("document should exist");

        assert_eq!(fetched.book_id, doc.book_id);
        assert_eq!(fetched.entitlement_id, doc.entitlement_id);
        assert_eq!(fetched.status_info.status, ReadStatus::InProgress);
        assert_eq!(fetched.status_info.times_started_reading, 1);

        let stats = fetched.statistics.expect("statistics should be present");
        assert_eq!(stats.remaining_time_minutes, Some(42));
        assert_eq!(stats.spent_reading_minutes, Some(58));

        let bookmark = fetched
            .current_bookmark
            .expect("bookmark should be present");
        assert_eq!(bookmark.progress_percent, Some(37.5));

        Ok(())
    }

    #[test(test)]
    fn update_persists_changes() -> Result<(), AppError> {
        let db = DocumentDB::open_in_memory()?;
        let mut doc = sample_doc("book-3");

        let id = db
            .create(DocumentTable::ReadingStates, &doc, None, None)
            .expect("create should succeed");

        doc.status_info.status = ReadStatus::Finished;
        doc.status_info.times_started_reading += 1;

        let updated = db
            .update(DocumentTable::ReadingStates, &id, &doc, None, None)
            .expect("update should succeed");
        assert!(
            updated,
            "update should report the doc was found and changed"
        );

        let fetched: ReadingStateDocument = db
            .read(DocumentTable::ReadingStates, &id)?
            .expect("document should still exist");

        assert_eq!(fetched.status_info.status, ReadStatus::Finished);
        assert_eq!(fetched.status_info.times_started_reading, 2);

        Ok(())
    }

    #[test(test)]
    fn delete_removes_document() -> Result<(), AppError> {
        let db = DocumentDB::open_in_memory()?;
        let doc = sample_doc("book-5");

        let id = db.create(DocumentTable::ReadingStates, &doc, None, None)?;

        let deleted =
            db.delete::<ReadingStateDocument>(DocumentTable::ReadingStates, &id, None, None)?;
        assert!(deleted);

        let fetched: Option<ReadingStateDocument> = db.read(DocumentTable::ReadingStates, &id)?;
        assert!(fetched.is_none());

        Ok(())
    }
}
