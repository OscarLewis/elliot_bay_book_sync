use crate::{
    database::document::{DocumentDB, DocumentTable},
    error::AppError,
    library::book::Book,
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
                        Ok(Some((existing_id, _existing_book))) => {
                            // TODO: Compare existing_book metadata/hash with scanned book to check if an update is needed
                            // For now, treat existing matches as skipped or queue for update:
                            // updated_books.push((existing_id.clone(), book.clone()));
                            skipped_books.push((existing_id, book));
                        }
                        Ok(None) => new_books.push(book),
                        Err(err) => error!(?err, path = ?book.path, "Failed to check path index"),
                    }
                }

                let added_count = new_books.len();
                let skipped_count = skipped_books.len();
                let updated_count = updated_books.len();

                // TODO: Process `updated_books` via `db.update(...)` once modification detection is implemented

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
