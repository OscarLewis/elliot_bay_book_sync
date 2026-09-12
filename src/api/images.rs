use crate::{
    AppState, api::make_requests::make_request_to_kobo_store, database::document::DocumentTable,
    error::AppError, library::book::Book,
};
use axum::{
    extract,
    http::{HeaderMap, Uri},
    response::{IntoResponse, Response},
};
use image::ImageFormat;
use reqwest::header;
use std::io::Cursor;
use tracing::{debug, error, info, warn};

pub(crate) async fn image_handler_with_quality(
    extract::State(state): extract::State<AppState>,
    extract::Path((token, book_uuid, width, height, quality, is_greyscale)): extract::Path<(
        String,
        String,
        u32,
        u32,
        String,
        String,
    )>,
    uri: Uri,
    method: reqwest::Method,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Response, AppError> {
    image_handler_inner(
        state,
        token,
        book_uuid,
        width,
        height,
        Some(quality),
        is_greyscale,
        uri,
        method,
        headers,
        body,
    )
    .await
}

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
    body: bytes::Bytes,
) -> Result<Response, AppError> {
    image_handler_inner(
        state,
        token,
        book_uuid,
        width,
        height,
        None,
        is_greyscale,
        uri,
        method,
        headers,
        body,
    )
    .await
}

pub(crate) async fn image_handler_inner(
    state: AppState,
    token: String,
    book_uuid: String,
    width: u32,
    height: u32,
    quality: Option<String>,
    is_greyscale: String,
    uri: Uri,
    method: reqwest::Method,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Response, AppError> {
    // TODO Proxy images of unknown books to Kobo store
    let book_res = match state
        .db
        .clone()
        .read::<Book>(DocumentTable::Books, &book_uuid)
    {
        Ok(book) => book,
        Err(app_error) => {
            error!(%app_error, %book_uuid, "Failed to read book for image request");
            None
        }
    };

    match book_res {
        Some(book) if book.has_image && book.image_path.is_some() => {
            debug!(
                book_uuid,
                width,
                height,
                title = book.title,
                book.image_path,
                "Image requested at size",
            );
            let is_greyscale = is_greyscale.eq_ignore_ascii_case("true");

            // TODO implment quality (JPEG image quality)
            // book-images/b07219a4-41c4-4a51-8024-d009488df748/300/569/90/False/the-eye-of-the-world-1.jpg
            if let Some(canonical_cover) = book.image()? {
                let mut cover_image =
                    canonical_cover.resize(width, u32::MAX, image::imageops::FilterType::Lanczos3);

                if is_greyscale {
                    cover_image = cover_image.grayscale();
                }

                let mut image_data = Cursor::new(Vec::new());
                cover_image.write_to(&mut image_data, ImageFormat::Jpeg)?;

                return Ok((
                    [(header::CONTENT_TYPE, "image/jpeg")],
                    image_data.into_inner(),
                )
                    .into_response());
            };
        }
        Some(_) => {
            warn!(%book_uuid, "Image requested for a book we have that's missing an image");
            return Err(AppError::NotFound("Image not found on server".into()));
        }
        None => {
            info!(%book_uuid, "Image requested for a book we don't have");
            // TODO Proxy this request to KOBO
            let resources = state.kobo_resources.lock().await;

            let image_url_template = &resources.image_url_template;
            let kobo_img_url = resources
                .image_url_template
                .replace("{ImageId}", &book_uuid.to_string())
                .replace("{Width}", &width.to_string())
                .replace("{Height}", &height.to_string());

            debug!(
                kobo_img_resource_url = kobo_img_url,
                "Attempting to proxy image request to Kobo CDN"
            );

            let response = make_request_to_kobo_store(
                &state.req_client,
                reqwest::Method::GET,
                &kobo_img_url,
                HeaderMap::new(),
                bytes::Bytes::new(),
            )
            .await?
            .error_for_status()?;

            let image = response.bytes().await?;

            return Ok(image.into_response());
        }
    }

    Ok(Response::new(axum::body::Body::empty()))
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
        let epub_path = PathBuf::from("test ebooks/The Lions of Al-Rassan - Guy Gavriel Kay.epub");

        assert!(
            epub_path.exists(),
            "Test EPUB fixture not found: {}",
            epub_path.display()
        );

        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;

        let db = DocumentDB::open_in_memory()?;

        let mut book = Book::from_path(epub_path);
        book.title = Some("The Lions of Al-Rassan - Guy Gavriel Kay".into());
        book.hardcover_id = Some(445946);
        book.hardcover_slug = Some("the-lions-of-al-rassan".to_string());
        book.hardcover_img_id = Some(463726);
        book.hardcover_img_url =
            Some("https://assets.hardcover.app/edition/16437269/33339-L.jpg".to_string());

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

        let image =
            image::load_from_memory_with_format(response.as_bytes(), image::ImageFormat::Jpeg)
                .expect("response should contain a valid JPEG");
        debug!(
            width = image.width(),
            height = image.height(),
            "Image recieved from server"
        );
        assert_eq!(image.width(), 300);

        // Delete old files
        for image_path in image_paths {
            std::fs::remove_file(image_path)?;
        }
        Ok(())
    }

    #[test(tokio::test)]
    async fn test_image_handler_proxy() -> Result<(), Box<dyn std::error::Error>> {
        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;

        let db = DocumentDB::open_in_memory()?;

        let state = AppState {
            config: Arc::new(config),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            kobo_resources: Arc::new(Mutex::new(Resources::default())),
            hardcover_api_token: None,
        };

        let server = setup_test_app(state);

        let token = "test-token-123";

        let book_id = "b07219a4-41c4-4a51-8024-d009488df748"; // The Eye of The World

        let response = server
            .get(&format!("/kobo/{token}/{book_id}/300/450/false/image.jpg"))
            .await;

        response.assert_status(StatusCode::OK);

        let image =
            image::load_from_memory_with_format(response.as_bytes(), image::ImageFormat::Jpeg)
                .expect("response should contain a valid JPEG");
        debug!(
            width = image.width(),
            height = image.height(),
            "Image recieved from server"
        );
        assert_eq!(image.width(), 300);

        Ok(())
    }
}
