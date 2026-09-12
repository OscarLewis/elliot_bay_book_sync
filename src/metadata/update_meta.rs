use strsim::jaro_winkler;
use tracing::debug;

use crate::{
    AppState,
    database::document::DocumentTable,
    error::AppError,
    library::book::Book,
    metadata::{
        epub::parse::parse_metadata_ebook, fetch_meta::fetch_metadata_for_book,
        match_results::match_metadata_for_book,
    },
};

pub async fn update_metadata(
    state: AppState,
    books_needing_metadata: Vec<(String, Book)>,
) -> Result<(), AppError> {
    for (id, mut book) in books_needing_metadata {
        let metadata = parse_metadata_ebook(book.path.clone().into()).await?;

        if let Some(token) = state.hardcover_api_token.as_deref() {
            debug!(
                book_doc_id = id,
                hardcover_api_enabled = true,
                "Updating metadata for book using Hardcover"
            );

            let search_results = fetch_metadata_for_book(token, &book.name, &metadata).await?;
            let result = match_metadata_for_book(&book.name, search_results, &metadata).await?;

            book.hardcover_id = Some(result.id);
            book.hardcover_slug = Some(result.slug);
            book.title = Some(result.title);
            // Check this author name against the one in the epub
            book.author = metadata
                .author
                .as_deref()
                .and_then(|epub_author| {
                    result
                        .author_names
                        .iter()
                        .max_by(|a, b| {
                            let a_score = jaro_winkler(epub_author, a);
                            let b_score = jaro_winkler(epub_author, b);
                            a_score.total_cmp(&b_score)
                        })
                        .cloned()
                })
                .or_else(|| result.author_names.first().cloned());
            book.has_metadata = true;
        } else {
            debug!(
                book_doc_id = id,
                hardcover_api_enabled = false,
                "Updating metadata for book using epub metadata"
            );
            book.title = metadata.title;
            book.author = metadata.author;
            book.has_metadata = true;
        }

        state.db.update(
            DocumentTable::Books,
            &id,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::document::{DocumentDB, DocumentTable};
    use crate::{AppState, config::AppConfig, library::book::Book};
    use dotenvy::dotenv;
    use std::env;
    use std::path::PathBuf;
    use std::sync::Arc;
    use test_log::test;
    use tracing::{debug, error};

    #[test(tokio::test)]
    async fn test_update_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        dotenv().ok();
        let hardcover_api_token = match env::var("HARDCOVER_TOKEN") {
            Ok(val) => {
                debug!("Hardcover Token loaded");
                Some(val)
            }
            Err(e) => {
                error!("Could not find HARDCOVER_TOKEN: {e}");
                None
            }
        };

        assert!(
            epub_path.exists(),
            "Test EPUB fixture not found: {}",
            epub_path.display()
        );

        let db = DocumentDB::open_in_memory()?;

        let book = Book::from_path(epub_path);

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let state = AppState {
            config: Arc::new(AppConfig::default()),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            hardcover_api_token,
        };

        update_metadata(state.clone(), vec![(book_id.clone(), book)]).await?;

        let updated: Book = state
            .db
            .read(DocumentTable::Books, &book_id)?
            .expect("Book should still exist");

        debug!(?updated, "Updated book");
        assert!(updated.has_metadata);

        Ok(())
    }
}
