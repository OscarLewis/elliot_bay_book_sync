use crate::{
    database::document::{DocumentDB, DocumentTable},
    error::AppError,
    library::book::Book,
    metadata::update_meta::update_metadata,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tokio::fs;
use tracing::{debug, error, info};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub scan_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDocument {
    pub status: ScanStatus,
    /// ISO-8601 formatted timestamp string (e.g., "2026-09-11T00:38:00Z")
    pub timestamp: String,
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
            timestamp: Utc::now().to_rfc3339(),
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
    db: Arc<DocumentDB>,
    scan_document_id: String,
    library_path: Arc<std::path::Path>,
) -> Result<(), AppError> {
    debug!(doc_id = %scan_document_id, "Initialized scan execution record");

    // Execute scan directly
    let scan_result: Result<(usize, usize, usize), AppError> =
        match scan_library(&library_path).await {
            Ok(book_list) => {
                let mut new_books = Vec::new();
                let mut updated_books: Vec<(String, Book)> = Vec::new();
                let mut skipped_books: Vec<(String, Book)> = Vec::new();

                for book in book_list {
                    match db.get_book_by_path::<_, Book>(&book.path) {
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
                        Err(err) => error!(?err, path = ?book.path, "Failed to check path index"),
                    }
                }

                let added_count = new_books.len();
                let skipped_count = skipped_books.len();
                let updated_count = updated_books.len();

                // Process `updated_books` via `db.update(...)` once modification detection is implemented
                for (id, book) in updated_books {
                    db.update(
                        DocumentTable::Books,
                        &id,
                        &book,
                        Some(|book: &Book| book.path.to_str().unwrap()),
                        None,
                    )?;
                }

                let batch_res = if !new_books.is_empty() {
                    db.create_many(
                        DocumentTable::Books,
                        &new_books,
                        Some(|b: &Book| b.path.to_str().unwrap_or_default()),
                        None,
                    )
                    .map(|_| {
                        info!(added_count, "Successfully batch-persisted new books");
                    })
                } else {
                    info!(
                        skipped_count,
                        updated_count, "No new books to insert; library is up to date"
                    );
                    Ok(())
                };

                batch_res.map(|_| (added_count, skipped_count, updated_count))
            }
            Err(err) => Err(err),
        };

    // Persist outcome
    match scan_result {
        Ok((added_count, skipped_count, updated_count)) => {
            let completed_record = ScanDocument {
                status: ScanStatus::Finished,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Completed {
                    added_count,
                    skipped_count,
                    updated_count,
                },
            };

            db.update(
                DocumentTable::Scans,
                &scan_document_id,
                &completed_record,
                None,
                Some(|s| s.timestamp.as_str()),
            )?;

            Ok(())
        }
        Err(err) => {
            let failed_record = ScanDocument {
                status: ScanStatus::Error,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Failed {
                    reason: err.to_string(),
                },
            };

            let _ = db.update(
                DocumentTable::Scans,
                &scan_document_id,
                &failed_record,
                None,
                Some(|s| s.timestamp.as_str()),
            );

            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_scan_updates_existing_book() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let epub_path = temp_dir.path().join("test.epub");

        // Create a file large enough for size_kb to be meaningful.
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        // Seed the database with the current book, but make its stored
        // metadata different from what is currently on disk.
        let mut existing_book = Book::from_path(epub_path.clone());
        existing_book.has_metadata = true;
        existing_book.size_kb = 1;
        existing_book.modified_at = "2000-01-01T00:00:00+00:00".to_string();

        let book_id = db.create(
            DocumentTable::Books,
            &existing_book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        // Run the scan. The filesystem version should be detected as changed.
        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(db.clone(), scan_id, temp_dir.path().to_path_buf().into()).await?;

        let updated: Book = db
            .read(DocumentTable::Books, &book_id)?
            .expect("Book should still exist");

        assert_eq!(updated.size_kb, 2);
        assert_ne!(updated.modified_at, "2000-01-01T00:00:00+00:00");
        assert!(!updated.has_metadata);

        Ok(())
    }

    #[tokio::test]
    async fn test_scan_adds_new_book() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("new.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(db.clone(), scan_id, temp_dir.path().to_path_buf().into()).await?;

        let books = db.get_all::<Book>(DocumentTable::Books)?;

        assert_eq!(books.len(), 1);

        let (_, book) = &books[0];
        assert_eq!(book.path, epub_path.into());
        assert_eq!(book.size_kb, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_scan_skips_unchanged_book() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("existing.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        let book = Book::from_path(epub_path.clone());

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(
            db.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let books = db.get_all::<Book>(DocumentTable::Books)?;

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].0, book_id);

        let scan: ScanDocument = db
            .read(DocumentTable::Scans, &scan_id)?
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

    #[tokio::test]
    async fn test_scan_updates_book_when_size_changes() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("changed.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        let mut book = Book::from_path(epub_path.clone());
        book.has_metadata = true;

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        tokio::fs::write(&epub_path, vec![1u8; 4 * 1024]).await?;

        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(
            db.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated: Book = db
            .read(DocumentTable::Books, &book_id)?
            .expect("Book should exist");

        assert_eq!(updated.size_kb, 4);
        assert!(!updated.has_metadata);

        let scan: ScanDocument = db
            .read(DocumentTable::Scans, &scan_id)?
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

    #[tokio::test]
    async fn test_scan_updates_book_when_modified_at_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("modified.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        let mut book = Book::from_path(epub_path.clone());
        book.has_metadata = true;
        book.modified_at = "2000-01-01T00:00:00+00:00".to_string();

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(
            db.clone(),
            scan_id.clone(),
            temp_dir.path().to_path_buf().into(),
        )
        .await?;

        let updated: Book = db
            .read(DocumentTable::Books, &book_id)?
            .expect("Book should exist");

        assert_ne!(updated.modified_at, "2000-01-01T00:00:00+00:00");
        assert!(!updated.has_metadata);

        let scan: ScanDocument = db
            .read(DocumentTable::Scans, &scan_id)?
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

    #[tokio::test]
    async fn test_scan_preserves_book_id_when_updated() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("preserve-id.epub");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let db = Arc::new(DocumentDB::open_in_memory()?);

        let book = Book::from_path(epub_path.clone());

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        tokio::fs::write(&epub_path, vec![1u8; 4 * 1024]).await?;

        let scan_id = db.create(
            DocumentTable::Scans,
            &ScanDocument {
                status: ScanStatus::Running,
                timestamp: Utc::now().to_rfc3339(),
                details: ScanDetails::Started,
            },
            None,
            Some(|s| s.timestamp.as_str()),
        )?;

        run_library_scan(db.clone(), scan_id, temp_dir.path().to_path_buf().into()).await?;

        let books = db.get_all::<Book>(DocumentTable::Books)?;

        assert_eq!(books.len(), 1);
        assert_eq!(books[0].0, book_id);

        Ok(())
    }
}
