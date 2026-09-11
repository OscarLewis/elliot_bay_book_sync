use crate::{
    database::document::{DocumentDB, DocumentTable},
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
    pub id: Option<Uuid>,
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
        skipped_count: usize,
    },
    Failed {
        reason: String,
    },
}

pub async fn scan_library(
    scan_id: Uuid,
    scan_dir: &Path,
) -> Result<(Uuid, Vec<Book>), Box<dyn std::error::Error>> {
    let mut book_list: Vec<Book> = vec![];
    if scan_dir.is_dir() {
        let mut entries = fs::read_dir(scan_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file()
                && (path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
                    || path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("kepub")))
            {
                // TODO handle 'Book Name.kepub.epub' files
                let book = Book::from_path(path);
                book_list.push(book);
            }
        }
    }
    if !book_list.is_empty() {
        debug!(number_epubs = book_list.len(), ?book_list, "Found files");
    }

    Ok((scan_id, book_list))
}

/// Background task that scans the library folder and batch-persists discovered books to redb
pub(crate) async fn run_library_scan(
    db: Arc<DocumentDB>,
    scan_id: Uuid,
    library_path: Arc<std::path::Path>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_scan_id, book_list) = scan_library(scan_id, &library_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut new_books = Vec::new();
    let mut skipped_books = Vec::new();

    // Use the O(1) index lookup per scanned book instead of loading all books into memory
    for book in book_list {
        match db.get_book_by_path::<_, Book>(&book.path) {
            Ok(Some((existing_id, _existing_book))) => {
                // Book already exists in index
                skipped_books.push((existing_id, book));
            }
            Ok(None) => {
                // New book, not present in path index
                new_books.push(book);
            }
            Err(err) => {
                error!(?err, path = ?book.path, "Failed to check path index for book");
            }
        }
    }

    let added_count = new_books.len();
    let skipped_count = skipped_books.len();

    if !skipped_books.is_empty() {
        debug!(
            skipped_count,
            "Skipped books that already existed in the database index; ready for updates"
        );

        // TODO: Iterate over `skipped_books` tuple (existing_id, updated_book) to perform updates
    }

    if !new_books.is_empty() {
        // Persist only newly discovered books in a single transaction
        if let Err(err) = db.create_many(
            DocumentTable::Books,
            &new_books,
            Some(|b: &Book| b.path.to_str().unwrap_or_default()),
            None,
        ) {
            error!(
                ?err,
                count = added_count,
                "Failed to save new books to database"
            );
        } else {
            info!(added_count, "Successfully batch-persisted new books");
        }
    } else {
        debug!("No new books found to persist.");
    }

    // Persist the scan execution record to SCAN_COLLECTION
    let record = ScanDocument {
        id: Some(scan_id),
        status: ScanStatus::Finished,
        timestamp: Utc::now().to_rfc3339(),
        details: ScanDetails::Completed {
            added_count,
            skipped_count,
        },
    };

    let record_doc_id = db
        .create(
            DocumentTable::Scans,
            &record,
            None,
            Some(|s| s.timestamp.as_str()),
        )
        .map_err(|e| e.to_string())?;

    debug!(
        %scan_id,
        doc_id = %record_doc_id,
        "Persisted scan record to database"
    );

    Ok(())
}
