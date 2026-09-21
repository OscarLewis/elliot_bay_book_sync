use crate::{
    AppState,
    error::AppError,
    library::book::Book,
    metadata::{
        epub::parse::parse_metadata_ebook, extract_images::extract_imgs_for_books,
        fetch_meta::fetch_metadata_for_book, match_results::match_metadata_for_book,
    },
};
use chrono::Utc;
use strsim::jaro_winkler;
use tracing::debug;

pub async fn update_metadata(
    state: AppState,
    books_needing_metadata: Vec<Book>,
) -> Result<(), AppError> {
    for mut book in books_needing_metadata {
        let metadata = parse_metadata_ebook(book.path.clone().into()).await?;
        debug!(
            book_name = %book.name,
            book_id = ?book.id,
            "Book loaded for metadata update"
        );

        let book_id = book.id.ok_or(AppError::InvalidObjectId)?;
        book.modified_at = Utc::now();

        if let Some(token) = state.hardcover_api_token.as_deref() {
            debug!(
                book_id = ?book_id,
                hardcover_api_enabled = true,
                "Updating metadata for book using Hardcover"
            );

            let search_results = fetch_metadata_for_book(token, &book.name, &metadata).await?;
            let result = match_metadata_for_book(&book.name, search_results, &metadata).await?;

            book.hardcover_id = Some(result.id);
            book.hardcover_slug = Some(result.slug);
            book.title = Some(result.title);
            book.description = result.description;

            book.hardcover_series_id = result
                .featured_series
                .as_ref()
                .and_then(|series| series.series.as_ref())
                .and_then(|series| series.id)
                .map(|id| id as u64);

            book.series_name = result
                .featured_series
                .as_ref()
                .and_then(|series| series.series.as_ref())
                .and_then(|series| series.name.clone());

            book.series_position = result
                .featured_series
                .as_ref()
                .and_then(|series| series.position);

            book.hardcover_img_id = result
                .image
                .as_ref()
                .and_then(|image| image.id as Option<u64>);

            book.hardcover_img_url = result.image.as_ref().and_then(|image| image.url.clone());

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
                book_id = ?book_id,
                hardcover_api_enabled = false,
                "Updating metadata for book using epub metadata"
            );

            book.title = metadata.title;
            book.author = metadata.author;
            book.has_metadata = true;
        }

        state.mongodb.books.update(book_id, &book).await?;
    }

    let books_needing_images = state.mongodb.books.find_books_needing_images().await?;

    extract_imgs_for_books(books_needing_images, state, true).await?;

    debug!("Finished metadata refresh");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig, library::book::Book, test_helpers::AppTestContext};
    use std::path::PathBuf;
    use test_context::test_context;
    use test_log::test;
    use tracing::debug;

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_update_metadata(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        assert!(
            epub_path.exists(),
            "Test EPUB fixture not found: {}",
            epub_path.display()
        );

        let mut book = Book::from_path(epub_path);

        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let state = ctx.state.clone();

        update_metadata(state.clone(), vec![book]).await?;

        let updated = state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should still exist");

        debug!(?updated, "Updated book");
        assert!(updated.has_metadata);

        Ok(())
    }
}
