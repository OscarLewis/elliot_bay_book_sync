use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    database::document::DocumentTable,
    error::AppError,
    library::{book::Book, sync::entitlement_models::BookMetadata},
};
use axum::{
    Json,
    body::Bytes,
    extract::{self, OriginalUri},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use reqwest::Method;
use tracing::{debug, info};

pub async fn metadata_request_handler(
    extract::Path((token, book_id)): extract::Path<(String, String)>,
    extract::State(state): extract::State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    info!(book_id, "Received Kobo metadata request");
    let book: Option<Book> = state.db.read(DocumentTable::Books, &book_id)?;

    let Some(book) = book else {
        debug!("Book not found in database, proxying request");
        let store_url = get_store_url_for_current_request(state.clone(), &uri, &token).await?;

        return Ok(redirect_or_proxy_request(
            &state.req_client,
            true,
            method,
            &store_url,
            headers,
            body,
            None,
        )
        .await
        .into_response());
    };
    let metadata_entitlement =
        BookMetadata::from_book_tuple((&book_id, &book), state.config.clone());

    info!(
        uri = uri.to_string(),
        book_id, "Received Kobo Metadata request"
    );

    Ok(Json(vec![metadata_entitlement]).into_response())
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::{book::Book, sync::entitlement_models::BookMetadata},
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::path::PathBuf;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_metadata_handler() -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "http://books.example.com/";
        let token = "test-token-123";
        let epub_path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        let config = AppConfig {
            base_url: test_base_url.to_string(),
            ebbooks_auth_key: token.to_string(),
            proxy_kobo_store: false,
            ..AppConfig::default()
        };

        let db = DocumentDB::open_in_memory()?;
        let state = AppState::new(config, db, None);
        let server = setup_test_app(state.clone());

        let book = Book::from_path(epub_path);

        let book_id = state.db.create(
            DocumentTable::Books,
            &book,
            Some(|book: &Book| book.path.to_str().unwrap()),
            None,
        )?;

        // Send metadata request to test server
        let metadata_response = server
            .get(&format!("/kobo/{token}/v1/library/{book_id}/metadata"))
            .await;

        metadata_response.assert_status(StatusCode::OK);

        Ok(())
    }
}
