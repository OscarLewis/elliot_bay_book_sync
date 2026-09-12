use std::path::{Path, PathBuf};

use tracing::debug;

use crate::{
    AppState, database::document::DocumentTable, error::AppError, library::book::Book,
    metadata::epub::parse::extract_epub_cover,
};

pub(crate) async fn extract_imgs_for_books(
    book_list: Vec<(String, Book)>,
    state: AppState,
    save_to_fs: bool,
) -> Result<Vec<PathBuf>, AppError> {
    debug!(
        ?book_list,
        state.config.image_path, "Extracting images for book list"
    );
    let mut image_paths = Vec::new();

    for (id, mut book) in book_list {
        let mut epub_image = None;

        if let Some(image) = extract_epub_cover(book.path.to_path_buf())? {
            debug!(
                width = image.width(),
                height = image.height(),
                book_doc_id = %id,
                book_name = %book.name,
                "Epub image stats"
            );

            epub_image = Some(image);
        }

        if let Some(img_url) = book.hardcover_img_url.clone() {
            let response = state
                .req_client
                .get(&img_url)
                .send()
                .await?
                .error_for_status()?;

            let bytes = response.bytes().await?;
            let hardcover_image = image::load_from_memory(&bytes)?;

            debug!(
                width = hardcover_image.width(),
                height = hardcover_image.height(),
                book_doc_id = %id,
                book_name = %book.name,
                "Hardcover image stats"
            );

            let image = if let Some(epub_image) = epub_image {
                let epub_pixels = u64::from(epub_image.width()) * u64::from(epub_image.height());

                let hardcover_pixels =
                    u64::from(hardcover_image.width()) * u64::from(hardcover_image.height());

                if hardcover_pixels > epub_pixels {
                    debug!(
                        epub_pixels,
                        hardcover_pixels,
                        book_doc_id = %id,
                        "Hardcover image is larger than EPUB image"
                    );

                    hardcover_image
                } else {
                    debug!(
                        epub_pixels,
                        hardcover_pixels,
                        book_doc_id = %id,
                        "EPUB image is larger than or equal to Hardcover image"
                    );

                    epub_image
                }
            } else {
                hardcover_image
            };

            if save_to_fs {
                let image_path = Path::new(&state.config.image_path).join(format!("{id}.webp"));

                book.image_path = Some(image_path.to_string_lossy().into_owned());
                book.has_image = true;

                state.db.update(
                    DocumentTable::Books,
                    &id,
                    &book,
                    Some(|book: &Book| book.path.to_str().unwrap()),
                    None,
                )?;
                let image_path = Path::new(&state.config.image_path).join(format!("{id}.webp"));

                image.save_with_format(&image_path, image::ImageFormat::WebP)?;

                image_paths.push(image_path);
            }
        }
    }

    Ok(image_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AppState,
        api::init_resources::Resources,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
    };
    use std::{path::PathBuf, sync::Arc};
    use test_log::test;
    use tokio::sync::Mutex;

    #[test(tokio::test)]
    async fn test_extract_imgs_for_books() -> Result<(), Box<dyn std::error::Error>> {
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        let db = DocumentDB::open_in_memory()?;

        let mut book = Book::from_path(epub_path);
        book.hardcover_id = Some(2168623);
        book.hardcover_slug = Some("absolute-martian-manhunter-vol-1".to_string());
        book.hardcover_img_id = Some(6034774);
        book.hardcover_img_url = Some("https://assets.hardcover.app/external_data/1633116/8fcc036b0ad1263a22eb5756ad5411a7f219a66a.jpeg".to_string());

        let _book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let book_list = db.get_all(DocumentTable::Books)?;

        let state = AppState {
            config: Arc::new(AppConfig::default()),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            hardcover_api_token: None,
            kobo_resources: Arc::new(Mutex::new(Resources::default())),
        };

        extract_imgs_for_books(book_list, state, false).await?;

        Ok(())
    }
}
