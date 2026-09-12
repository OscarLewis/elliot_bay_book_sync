use crate::{AppState, database::document::DocumentTable, library::book::Book};
use axum::{
    body::Body,
    extract,
    http::{HeaderMap, Uri},
    response::Response,
};
use bytes::Bytes;
use reqwest::StatusCode;
use tracing::debug;

pub(crate) async fn image_handler(
    extract::State(state): extract::State<AppState>,
    extract::Path((token, book_uuid, width, height, is_greyscale)): extract::Path<(
        String,
        String,
        u32,
        u32,
        String,
    )>,
    uri: Uri,
    method: reqwest::Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // TODO Proxy images of unknown books to Kobo store
    let book = match state
        .db
        .clone()
        .read::<Book>(DocumentTable::Books, &book_uuid)
    {
        Ok(Some(book)) => book,
        Ok(None) => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }
        Err(error) => {
            tracing::error!(%error, %book_uuid, "Failed to read book for image request");

            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap();
        }
    };
    let image_path = match book.image_path.clone() {
        Some(path) => path,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }
    };
    debug!(
        book_uuid,
        width,
        height,
        title = book.title,
        image_path,
        "Image requested at size",
    );
    Response::new(axum::body::Body::empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::init_resources::Resources,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::book::Book,
        metadata::extract_images::extract_imgs_for_books,
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::{path::PathBuf, sync::Arc};
    use test_log::test;
    use tokio::sync::Mutex;

    #[test(tokio::test)]
    async fn test_image_handler() -> Result<(), Box<dyn std::error::Error>> {
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        assert!(
            epub_path.exists(),
            "Test EPUB fixture not found: {}",
            epub_path.display()
        );

        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;

        let db = DocumentDB::open_in_memory()?;

        let mut book = Book::from_path(epub_path);
        book.title = Some("Absolute Martian Manhunter, Vol. 1: Martian Vision".into());
        book.hardcover_id = Some(2168623);
        book.hardcover_slug = Some("absolute-martian-manhunter-vol-1".to_string());
        book.hardcover_img_id = Some(6034774);
        book.hardcover_img_url = Some(
        "https://assets.hardcover.app/external_data/1633116/8fcc036b0ad1263a22eb5756ad5411a7f219a66a.jpeg"
            .to_string(),
    );

        let book_id = db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let book_list = db.get_all(DocumentTable::Books)?;

        let state = AppState {
            config: Arc::new(config),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            kobo_resources: Arc::new(Mutex::new(Resources::default())),
            hardcover_api_token: None,
        };

        let image_paths = extract_imgs_for_books(book_list, state.clone(), true).await?;

        let updated: Book = state
            .db
            .read(DocumentTable::Books, &book_id)?
            .expect("Book should still exist");

        assert!(updated.has_image);
        assert!(updated.image_path.is_some());

        let server = setup_test_app(state);

        let token = "test-token-123";

        let response = server
            .get(&format!("/kobo/{token}/{book_id}/300/450/false/image.jpg"))
            .await;

        response.assert_status(StatusCode::OK);

        // Delete old files
        for image_path in image_paths {
            std::fs::remove_file(image_path)?;
        }
        Ok(())
    }
}
