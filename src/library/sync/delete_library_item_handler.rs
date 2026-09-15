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
    let synced_books: Vec<(String, SyncedBookDocument)> =
        state.db.get_all(DocumentTable::SyncedBooks)?;

    if let Some((doc_key, _)) = synced_books
        .iter()
        .find(|(_, doc)| doc.book_id == book_id.as_ref())
    {
        debug!(book_id, "Deleting sync status for book");
        state
            .db
            .delete::<SyncedBookDocument>(DocumentTable::SyncedBooks, doc_key, None, None)?;
    }
    Ok(StatusCode::OK.into_response())
}
