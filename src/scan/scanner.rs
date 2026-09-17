use crate::database::bson_chrono_datetime::bson_chrono_datetime;
use crate::{
    database::{
        MongoDatabase,
        document::{DocumentDB, DocumentTable},
    },
    error::AppError,
    library::book::Book,
};
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::{debug, error, info};
use uuid::Uuid;
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub scan_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDocument {
    pub status: ScanStatus,

    #[serde(with = "bson_chrono_datetime")]
    pub timestamp: DateTime<Utc>,

    pub details: ScanDetails,
}

impl ScanDocument {
    /// Builds a new scan document in the initial `Running`/`Started` state,
    /// with `timestamp` set to the current time.
    ///
    /// This is the only place a `Running` scan should be constructed —
    /// callers that want to record a new scan starting should use this
    /// (via `ScanRepository::start`) rather than building a `ScanDocument`
    /// by hand, so the `Running`/`Started` pairing and timestamp format stay
    /// consistent everywhere a scan begins.
    pub fn start() -> Self {
        Self {
            status: ScanStatus::Running,
            timestamp: Utc::now(),
            details: ScanDetails::Started,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Running,
    Error,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ScanDetails {
    Started,
    Completed {
        added_count: usize,
        updated_count: usize,
        skipped_count: usize,
    },
    Failed {
        reason: String,
    },
}

pub async fn scan_library(scan_dir: &Path) -> Result<Vec<Book>, AppError> {
    let mut book_list: Vec<Book> = vec![];
    if scan_dir.is_dir() {
        let mut entries = fs::read_dir(scan_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let is_supported = matches!(path.extension(), Some(ext) if ext == "epub" || ext == "kepub")
                    || (
                        // Check to see if a file as a title like 'Book Name.kepub.epub' - that's supported just fine
                        path.file_stem()
                            .and_then(|stem| Path::new(stem).extension())
                            .is_some_and(|ext| ext == "kepub")
                            && path.extension().is_some_and(|ext| ext == "epub")
                    );

                if is_supported {
                    let book = Book::from_path(path);
                    book_list.push(book);
                }
            }
        }
    }
    if !book_list.is_empty() {
        debug!(number_epubs = book_list.len(), ?book_list, "Found files");
    }

    Ok(book_list)
}

pub(crate) async fn run_library_scan(
    mongodb: Arc<MongoDatabase>,
    scan_document_id: ObjectId,
    library_path: Arc<std::path::Path>,
) -> Result<(), AppError> {
    debug!(doc_id = %scan_document_id, "Initialized scan execution record");

    // TODO Handle missing books as deleted books or flag them in mongodb

    // TODO take in an ObjectId for a mongodb.scans instead of a string for scan_document_id

    // Execute scan directly
    let scan_result: Result<(usize, usize, usize), AppError> =
        match scan_library(&library_path).await {
            Ok(book_list) => {
                let mut new_books = Vec::new();
                let mut updated_books: Vec<(ObjectId, Book)> = Vec::new();
                let mut skipped_books: Vec<(ObjectId, Book)> = Vec::new();

                for book in book_list {
                    let book_path_str = book
                        .path
                        .to_str()
                        .ok_or_else(|| AppError::InvalidPath("Invalid book path".into()))?;

                    match mongodb.books.find_by_path(book_path_str).await {
                        Ok(Some((existing_id, existing_book))) => {
                            if existing_book.size_kb != book.size_kb
                                || existing_book.modified_at != book.modified_at
                            {
                                let mut book = book;
                                book.has_metadata = false;

                                updated_books.push((existing_id, book));
                            } else {
                                skipped_books.push((existing_id, book));
                            }
                        }
                        Ok(None) => new_books.push(book),
                        Err(err) => {
                            error!(?err, path = ?book.path, "Failed to check path index");
                        }
                    }
                }

                let added_count = new_books.len();
                let skipped_count = skipped_books.len();
                let updated_count = updated_books.len();

                for (id, book) in updated_books {
                    mongodb.books.update_diff(id, &book).await?;
                }

                let batch_res = if !new_books.is_empty() {
                    mongodb.books.insert_many(&mut new_books).await.map(|ids| {
                        info!(
                            added_count = ids.len(),
                            "Successfully batch-persisted new books to MongoDB"
                        );

                        ids.into_iter()
                            .map(|id| id.to_hex())
                            .collect::<Vec<String>>()
                    })
                } else {
                    info!(
                        skipped_count,
                        updated_count, "No new books to insert; library is up to date"
                    );

                    Ok(Vec::new())
                };

                batch_res.map(|_| (added_count, skipped_count, updated_count))
            }
            Err(err) => Err(err),
        };

    // Persist outcome
    match scan_result {
        Ok((added_count, skipped_count, updated_count)) => {
            mongodb
                .scans
                .mark_completed(scan_document_id, added_count, updated_count, skipped_count)
                .await?;

            Ok(())
        }
        Err(err) => {
            mongodb
                .scans
                .mark_failed(scan_document_id, err.to_string())
                .await?;

            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::ScanRepository, test_helpers::AppTextContext};
    use tempfile::tempdir;
    use test_context::test_context;

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_updates_existing_book(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("test.epub");

        // Create a file large enough for size_kb to be meaningful.
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        // Seed the database with the current book, but make its stored
        // metadata different from what is currently on disk.
        let mut existing_book = Book::from_path(epub_path.clone());
        existing_book.has_metadata = true;
        existing_book.size_kb = 1;
        existing_book.modified_at = DateTime::parse_from_rfc3339("2000-01-01T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        let book_id = ctx.state.mongodb.books.insert(&mut existing_book).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id,
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated = ctx
            .state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        assert_eq!(updated.size_kb, 2);
        assert_ne!(
            updated.modified_at,
            DateTime::parse_from_rfc3339("2000-01-01T00:00:00+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert!(!updated.has_metadata);

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_adds_new_book(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("new.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id,
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let (_, book) = ctx
            .state
            .mongodb
            .books
            .find_by_path(epub_path.to_str().unwrap())
            .await?
            .expect("Book should exist in database");

        assert_eq!(book.path, epub_path.into());
        assert_eq!(book.size_kb, 2);

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_skips_unchanged_book(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("existing.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mut book = Book::from_path(epub_path.clone());
        book.size_kb = 2;
        book.modified_at = std::fs::metadata(&epub_path)?.modified()?.into();

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let books = ctx
            .state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        assert_eq!(books.path, epub_path.into());
        assert_eq!(books.size_kb, 2);
        assert_eq!(books.has_metadata, book.has_metadata);

        let scan = ctx
            .state
            .mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("Scan should exist");

        assert_eq!(
            scan.details,
            ScanDetails::Completed {
                added_count: 0,
                updated_count: 0,
                skipped_count: 1,
            }
        );

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_updates_book_when_size_changes(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("changed.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mut book = Book::from_path(epub_path.clone());
        book.has_metadata = true;

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        tokio::fs::write(&epub_path, vec![1u8; 4 * 1024]).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated = ctx
            .state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        assert_eq!(updated.size_kb, 4);
        assert!(!updated.has_metadata);

        let scan = ctx
            .state
            .mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("Scan should exist");

        assert_eq!(
            scan.details,
            ScanDetails::Completed {
                added_count: 0,
                updated_count: 1,
                skipped_count: 0,
            }
        );

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_updates_book_when_modified_at_changes(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("modified.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mut book = Book::from_path(epub_path.clone());
        book.has_metadata = true;
        book.modified_at = DateTime::parse_from_rfc3339("2000-01-01T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated = ctx
            .state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        assert_ne!(
            updated.modified_at,
            DateTime::parse_from_rfc3339("2000-01-01T00:00:00+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert!(!updated.has_metadata);

        let scan = ctx
            .state
            .mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("Scan should exist");

        assert_eq!(
            scan.details,
            ScanDetails::Completed {
                added_count: 0,
                updated_count: 1,
                skipped_count: 0,
            }
        );

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[tokio::test]
    async fn test_scan_preserves_book_id_when_updated(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("preserve-id.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mut book = Book::from_path(epub_path.clone());

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        tokio::fs::write(&epub_path, vec![1u8; 4 * 1024]).await?;

        let scan_id = ctx.state.mongodb.scans.start().await?;

        run_library_scan(
            ctx.state.mongodb.clone(),
            scan_id,
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated = ctx
            .state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        assert_eq!(updated.path, epub_path.into());
        assert_eq!(updated.size_kb, 4);

        Ok(())
    }
}
