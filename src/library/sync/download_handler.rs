use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    error::AppError,
    library::{
        book::Book,
        sync::entitlement_models::{BookMetadata, KoboFormat},
    },
};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{self, OriginalUri},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use mongodb::bson::oid::ObjectId;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use tracing::{debug, info, warn};

// `/kobo/{token}/download/{book_id}/{book_format}`
pub async fn download_request_handler(
    extract::Path((token, book_id, book_format)): extract::Path<(String, String, String)>,
    extract::State(state): extract::State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    info!(book_id, book_format, "Received Kobo download request");
    let book_id = ObjectId::parse_str(&book_id).map_err(|_| AppError::InvalidObjectId)?;
    let book_opt: Option<Book> = state.mongodb.books.find_by_id(book_id).await?;
    let Some(book) = book_opt else {
        warn!("Book not found in database while attempting to download to device");
        return Err(AppError::NotFound("Book not found in database".into()));
    };

    let format = match book_format.as_str() {
        value if value.eq_ignore_ascii_case(KoboFormat::Kepub.download_format()) => {
            KoboFormat::Kepub
        }
        value if value.eq_ignore_ascii_case(KoboFormat::Epub.download_format()) => KoboFormat::Epub,
        _ => {
            return Err(AppError::NotFound("Unsupported Kobo book format".into()));
        }
    };

    // Book is avaliable here
    // book.path is a Box<Path> pointing to the file on disk

    let filename = match format {
        KoboFormat::Kepub => book
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("{}.kepub.epub", name.trim_end_matches(".kepub")))
            .ok_or_else(|| AppError::NotFound("Invalid book filename".into()))?,
        KoboFormat::Epub => book
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::NotFound("Invalid book filename".into()))?
            .to_string(),
    };

    // Just need logic to match incoming format to KoboFormat

    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::from(body))?;

    *request.headers_mut() = headers;

    let response = ServeFile::new(&book.path)
        .oneshot(request)
        .await
        .into_response();

    debug!(
        ?book_id,
        format = ?format,
        filename = %filename,
        path = %book.path.display(),
        "Serving Kobo book download"
    );

    Ok(response.into_response())

    // TODO implement download handler
}

#[cfg(test)]
mod tests {
    use crate::{
        library::{
            book::Book,
            sync::entitlement_models::{BookMetadata, KoboFormat},
        },
        test_helpers::{AppTextContext, setup_test_app},
    };
    use reqwest::StatusCode;
    use std::path::PathBuf;
    use test_context::test_context;
    use test_log::test;

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    async fn test_download_handler(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "http://books.example.com/";
        let token = "test-token-123";
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        let book_format = KoboFormat::Epub;
        let book_format_str = book_format.download_format();

        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = test_base_url.to_string();
        config.ebbooks_auth_key = token.to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let server = setup_test_app(state.clone());

        let mut book = Book::from_path(epub_path.clone());

        let book_id = state.mongodb.books.insert(&mut book).await?;

        let expected_body = tokio::fs::read(&epub_path).await?;

        let download_response = server
            .get(&format!(
                "/kobo/{token}/download/{book_id}/{book_format_str}"
            ))
            .await;

        download_response.assert_status(StatusCode::OK);

        let body = download_response.into_bytes();
        assert_eq!(body.as_ref(), expected_body.as_slice());

        state.mongodb.books.delete(book_id).await?;

        Ok(())
    }
}
