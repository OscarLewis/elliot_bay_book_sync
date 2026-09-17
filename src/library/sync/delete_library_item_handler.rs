use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    database::document::DocumentTable,
    error::AppError,
    library::{
        book::Book,
        sync::{entitlement_models::BookMetadata, sync_document::SyncedBookDocument},
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::{self, OriginalUri},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use reqwest::Method;
use tracing::{debug, info};

pub async fn library_item_delete_handler(
    extract::Path((token, book_id)): extract::Path<(String, String)>,
    extract::State(state): extract::State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    debug!(book_id, "Deleting sync status for book");

    state.mongodb.syncs.delete_by_book_id(&book_id).await?;

    Ok(StatusCode::OK.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        library::sync::sync_document::SyncedBookDocument,
        test_helpers::{AppTextContext, setup_test_app},
    };
    use test_context::test_context;
    use test_log::test;

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    async fn test_library_item_delete_handler(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = "test-token-123";
        let book_id = "test-book-id";

        let mut synced_book = SyncedBookDocument {
            book_id: book_id.to_string(),
            user_id: "default".to_string(),
            id: None,
            synced_at: chrono::Utc::now(),
        };

        ctx.state.mongodb.syncs.insert(&mut synced_book).await?;

        let server = setup_test_app(ctx.state.clone());

        let response = server
            .delete(&format!("/kobo/{token}/v1/library/{book_id}"))
            .await;

        response.assert_status(StatusCode::OK);

        let synced_books = ctx.state.mongodb.syncs.fetch_all().await?;

        assert!(
            synced_books
                .iter()
                .all(|synced_book| synced_book.book_id != book_id)
        );

        Ok(())
    }
}
