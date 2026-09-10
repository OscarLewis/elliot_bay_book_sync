use crate::{
    library::book::Book,
    scan::status::{ScanStatus, ScanStatusMessage},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub scan_id: Uuid,
}

pub async fn scan_library(
    status: ScanStatus,
    scan_id: Uuid,
    scan_dir: &Path,
) -> Result<(Uuid, Vec<Book>), Box<dyn std::error::Error>> {
    status.send(ScanStatusMessage::ScanStarted {
        scan_id,
        path: scan_dir.to_path_buf(),
    });
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
