use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    database::document::DocumentTable,
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
    let book_opt: Option<Book> = state.db.read(DocumentTable::Books, &book_id)?;
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

    let file = tokio::fs::File::open(&book.path)
        .await
        .map_err(AppError::Io)?;

    let metadata = tokio::fs::metadata(&book.path)
        .await
        .map_err(AppError::Io)?;

    let file = tokio::fs::File::open(&book.path)
        .await
        .map_err(AppError::Io)?;

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
        book_id,
        format = ?format,
        filename = %filename,
        path = %book.path.display(),
        "Serving Kobo book download"
    );

    Ok(response.into_response())

    // TODO implement download handler
}

/*
    @kobo.route("/download/<book_id>/<book_format>")
    @requires_kobo_auth
    @download_required
    def download_book(book_id, book_format):
        return get_download_link(book_id, book_format, "kobo")


*/

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::{
            book::Book,
            sync::entitlement_models::{BookMetadata, KoboFormat},
        },
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::path::PathBuf;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_download_handler() -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "http://books.example.com/";
        let token = "test-token-123";
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        let book_format = KoboFormat::Epub;
        let book_format_str = book_format.download_format();

        let config = AppConfig {
            base_url: test_base_url.to_string(),
            ebbooks_auth_key: token.to_string(),
            proxy_kobo_store: false,
            ..AppConfig::default()
        };

        let db = DocumentDB::open_in_memory()?;
        let state = AppState::new(config, db, None);
        let server = setup_test_app(state.clone());

        let book = Book::from_path(epub_path.clone());

        let book_id = state.db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        let expected_body = tokio::fs::read(&epub_path).await?;
        let expected_filename = epub_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap();

        let download_response = server
            .get(&format!(
                "/kobo/{token}/download/{book_id}/{book_format_str}"
            ))
            .await;

        download_response.assert_status(StatusCode::OK);

        let content_disposition = download_response
            .headers()
            .get("content-disposition")
            .expect("Content-Disposition header should be present")
            .to_str()?;

        assert_eq!(
            content_disposition,
            format!("attachment; filename=\"{expected_filename}\"")
        );

        let body = download_response.into_bytes();

        assert_eq!(body.as_ref(), expected_body.as_slice());

        Ok(())
    }
}
