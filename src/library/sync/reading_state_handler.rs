//! Handle routing for `/kobo/{token}/v1/library/{book_uuid}/state`
//! This is either a PUT request to update our database with the latest Reading State
//! or a GET request
//! of a book.

use crate::{AppState, error::AppError};
use axum::{
    extract,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;

pub async fn reading_state_handler(
    extract::Path((token, book_id)): extract::Path<(String, String)>,
    extract::State(state): extract::State<AppState>,
) -> Result<Response, AppError> {
    // Handles both GET and PUT
    // TODO Finish implementing reading state handler
    Ok(StatusCode::OK.into_response())
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::book::Book,
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::path::PathBuf;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_store_reading_state() -> Result<(), Box<dyn std::error::Error>> {
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

        // Scan first to populate the library
        let scan_response = server
            .put(&format!("/kobo/{token}/v1/library/{book_id}/state"))
            .await;

        scan_response.assert_status(StatusCode::OK);

        Ok(())
    }
}
