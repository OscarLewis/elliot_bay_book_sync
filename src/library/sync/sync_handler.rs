use crate::{
    AppState,
    api::make_requests::{get_download_url_format_for_book, make_request_to_kobo_store},
    error::AppError,
    library::{
        book::Book,
        sync::{
            entitlement_models::{Entitlement, SyncResult},
            sync_document::SyncedBookDocument,
            sync_token::{SYNC_TOKEN_HEADER, SyncToken},
        },
    },
};
use axum::{
    extract::{self},
    http::{HeaderMap, uri},
    response::{IntoResponse, Response},
};
use chrono::{TimeZone, Utc};
use std::collections::{HashMap, HashSet};
use tracing::{debug, error};

pub const SYNC_ITEM_LIMIT: usize = 100;

pub async fn library_sync_handler(
    uri: uri::Uri,
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // Convert HeaderMap to HashMap<String, String>
    let headers_map: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_string())))
        .collect();
    debug!(
        orgin_uri = &uri.to_string(),
        headers = ?headers_map,
        "Sync request received"
    );
    let mut sync_token = SyncToken::from_headers(&headers_map);
    debug!(?sync_token, "Sync token generated from headers");
    let url_format = get_download_url_format_for_book(&state.config.base_url, &token);
    debug!(url_format, "Download link format");

    // Fetch all synced book records and library books
    let synced_books = state.mongodb.syncs.fetch_all().await?;
    let books = state.mongodb.books.fetch_all().await?;

    // TODO WIP FLAG FOR Mongodb re-write - PICK UP FROM HERE

    // If we have no synced books, we disrespect the SyncToken
    // His shoes wack
    if synced_books.is_empty() {
        debug!("No previously synced ebooks found, disregarding SyncToken");

        sync_token.data.books_last_modified = Utc.timestamp_opt(0, 0).unwrap();
        sync_token.data.books_last_created = Utc.timestamp_opt(0, 0).unwrap();
        sync_token.data.reading_state_last_modified = Utc.timestamp_opt(0, 0).unwrap();
    }

    let mut sync_results: Vec<SyncResult> = Vec::new();
    let mut new_books_last_modified = sync_token.data.books_last_modified;
    let mut new_books_last_created = sync_token.data.books_last_created;
    let mut new_reading_state_last_modified = sync_token.data.reading_state_last_modified;
    let mut new_archived_last_modified = Utc.timestamp_opt(0, 0).unwrap();

    // Track whether this sync involves any locally-added books.
    // Local books are never registered with Kobo's real catalog, so a
    // sync that includes them should not be validated/proxied upstream —
    // Kobo's store has no record of them and will reject the request.
    let mut has_local_books = false;

    // Handle sync logic by comparing synced books ids versus the ids in our database
    let synced_book_ids: HashSet<String> =
        synced_books.iter().map(|sb| sb.book_id.clone()).collect();

    let allowed_book_ids: HashSet<String> = books
        .iter()
        .filter_map(|book| book.id.map(|id| id.to_string()))
        .collect();

    let books_to_delete_ids: HashSet<String> = synced_book_ids
        .difference(&allowed_book_ids)
        .cloned()
        .collect();

    if !books_to_delete_ids.is_empty() {
        debug!(
            "Kobo Sync: found {} books to remove from device",
            books_to_delete_ids.len()
        );
        has_local_books = true;

        for book_id in &books_to_delete_ids {
            state.mongodb.syncs.delete_by_book_id(book_id).await?;
        }
    }

    let books_to_sync: Vec<&Book> = books
        .iter()
        .filter(|book| {
            book.id
                .map(|id| !synced_book_ids.contains(&id.to_string()))
                .unwrap_or(false)
        })
        .take(SYNC_ITEM_LIMIT)
        .collect();

    debug!(
        num_books_to_sync = books_to_sync.len(),
        "Found books to sync"
    );

    for book in books_to_sync {
        let book_id = book.id.ok_or(AppError::InvalidObjectId)?.to_string();

        has_local_books = true;

        let book_modified: chrono::DateTime<Utc> = book.modified_at;
        let is_new = book_modified > sync_token.data.books_last_created;

        let entitlement = Entitlement::from_book_tuple(
            (&book_id, book),
            state.config.clone(),
            None,
            None,
            None,
            false,
        );

        if is_new {
            sync_results.push(SyncResult {
                changed_entitlement: None,
                new_entitlement: Some(entitlement),
                changed_reading_state: None,
                deleted_tag: None,
                new_tag: None,
                changed_tag: None,
            });
        } else {
            sync_results.push(SyncResult {
                changed_entitlement: Some(entitlement),
                new_entitlement: None,
                changed_reading_state: None,
                deleted_tag: None,
                new_tag: None,
                changed_tag: None,
            });
        }

        debug!(
            book_id = %book_id,
            title = book.title,
            is_new,
            "Attempting to sync book"
        );

        new_books_last_modified = std::cmp::max(new_books_last_modified, book_modified);
        new_books_last_created = std::cmp::max(new_books_last_created, book_modified);

        let mut synced = SyncedBookDocument {
            book_id,
            user_id: "default".to_string(),
            id: None,
            synced_at: Utc::now(),
        };

        state.mongodb.syncs.insert(&mut synced).await?;
    }

    // Update sync token
    sync_token.data.books_last_modified = new_books_last_modified;
    sync_token.data.books_last_created = new_books_last_created;
    sync_token.data.archive_last_modified = new_archived_last_modified;

    // Only proxy to Kobo's real store when this sync doesn't touch local
    // content — proxying a sync that contains locally-added books will
    // fail with RequestBindingException, since Kobo has no record of them.
    let should_proxy = state.config.proxy_kobo_store && !has_local_books;

    generate_sync_response(&mut sync_token, sync_results, false, &state, should_proxy).await
}

pub async fn generate_sync_response(
    sync_token: &mut SyncToken,
    sync_results: Vec<SyncResult>,
    set_cont: bool,
    state: &AppState,
    should_proxy: bool,
) -> Result<Response, AppError> {
    let mut response_headers = HeaderMap::new();
    let resources = state.kobo_resources.lock().await;
    let mut final_results = sync_results;

    let kobo_sync_endpoint = &resources.library_sync;

    // Optionally merge results from Kobo store
    if should_proxy && !set_cont {
        match make_request_to_kobo_store(
            &state.req_client,
            reqwest::Method::POST,
            kobo_sync_endpoint,
            response_headers.clone(),
            serde_json::to_vec(sync_token)?.into(),
            Some(sync_token),
        )
        .await
        {
            Ok(store_response) => {
                // Extract headers before consuming response
                let sync_header = store_response.headers().get("x-kobo-sync").cloned();
                let sync_mode = store_response.headers().get("x-kobo-sync-mode").cloned();
                let recent_reads = store_response.headers().get("x-kobo-recent-reads").cloned();

                let store_headers: HashMap<String, String> = store_response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();

                // Read body as text first so we can log it when it isn't what we expect
                let body_text = match store_response.text().await {
                    Ok(t) => t,
                    Err(e) => {
                        error!(?e, "Failed to read Kobo store response body");
                        String::new()
                    }
                };

                match serde_json::from_str::<serde_json::Value>(&body_text) {
                    Ok(serde_json::Value::Array(items)) => {
                        for val in items {
                            match serde_json::from_value::<SyncResult>(val) {
                                Ok(sync_result) => final_results.push(sync_result),
                                Err(e) => debug!(?e, "Skipping unrecognized store sync result"),
                            }
                        }
                    }
                    Ok(other) => {
                        error!(body = %other, "Kobo store returned non-array sync response");
                    }
                    Err(e) => {
                        error!(
                            ?e,
                            body = %body_text.chars().take(500).collect::<String>(),
                            "Failed to parse Kobo store response as JSON"
                        );
                    }
                }

                // Copy headers from store response regardless of body shape
                if let Some(header) = sync_header {
                    response_headers.insert(
                        axum::http::HeaderName::from_static("x-kobo-sync"),
                        header.clone(),
                    );
                }
                if let Some(header) = sync_mode {
                    response_headers.insert(
                        axum::http::HeaderName::from_static("x-kobo-sync-mode"),
                        header.clone(),
                    );
                }
                if let Some(header) = recent_reads {
                    response_headers.insert(
                        axum::http::HeaderName::from_static("x-kobo-recent-reads"),
                        header.clone(),
                    );
                }

                // Merge store response token
                sync_token.merge_from_store_response(&store_headers);
            }
            Err(e) => {
                error!(?e, "Failed to receive response from Kobo's sync endpoint");
            }
        }
    }

    // Add continuation header if needed
    if set_cont {
        response_headers.insert(
            axum::http::HeaderName::from_static("x-kobo-sync"),
            axum::http::HeaderValue::from_static("continue"),
        );
    }

    // Add sync token to response headers
    let mut token_headers = HashMap::new();
    sync_token.to_headers(&mut token_headers);
    if let Some(token_value) = token_headers.get(SYNC_TOKEN_HEADER) {
        response_headers.insert(
            axum::http::HeaderName::from_static(SYNC_TOKEN_HEADER),
            axum::http::HeaderValue::from_str(token_value)?,
        );
    }

    debug!(books_synced = final_results.len(), "Kobo sync completed");

    // Build JSON response with proper encoding
    let json_body = serde_json::to_string(&final_results)?;
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json; charset=utf-8"),
    );

    Ok((response_headers, json_body).into_response())
}

#[cfg(test)]
mod tests {
    use crate::{
        library::book::Book,
        library::sync::{sync_handler::generate_sync_response, sync_token::SyncToken},
        metadata::update_meta::update_metadata,
        test_helpers::{AppTextContext, setup_test_app},
    };
    use reqwest::StatusCode;
    use test_context::test_context;
    use test_log::test;
    use tokio::time::{Duration, sleep};

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_library_sync(ctx: &mut AppTextContext) -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "http://books.example.com/";
        let token = "test-token-123";

        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = test_base_url.to_string();
        config.ebbooks_auth_key = token.to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let server = setup_test_app(state.clone());

        // Scan first to populate the library
        let scan_response = server.post("/scan").await;
        scan_response.assert_status(StatusCode::OK);

        sleep(Duration::from_millis(500)).await;

        // Fetch all books and update metadata
        let books_needing_metadata = state.mongodb.books.fetch_all().await?;
        update_metadata(state.clone(), books_needing_metadata).await?;

        // Wait for metadata update and image extraction to complete
        sleep(Duration::from_millis(2500)).await;

        // Now sync
        let response = server.get(&format!("/kobo/{token}/v1/library/sync")).await;
        response.assert_status(StatusCode::OK);

        // Cleanup: remove all downloaded images
        let all_books = state.mongodb.books.fetch_all().await?;
        for book in all_books {
            if let Some(image_path) = book.image_path {
                let _ = tokio::fs::remove_file(&image_path).await;
            }
        }

        Ok(())
    }

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_no_proxy(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&std::collections::HashMap::new());
        let sync_results = vec![];

        let response =
            generate_sync_response(&mut sync_token, sync_results, false, &state, false).await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_with_continuation(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&std::collections::HashMap::new());
        let sync_results = vec![];

        let response =
            generate_sync_response(&mut sync_token, sync_results, true, &state, false).await?;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get("x-kobo-sync")
                .is_some_and(|v| v == "continue")
        );
        Ok(())
    }

    #[test_context(AppTextContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_includes_token(
        ctx: &mut AppTextContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&std::collections::HashMap::new());
        let sync_results = vec![];

        let response =
            generate_sync_response(&mut sync_token, sync_results, false, &state, false).await?;

        assert!(response.headers().get("x-kobo-synctoken").is_some());
        assert_eq!(
            response.headers().get("content-type").unwrap().to_str()?,
            "application/json; charset=utf-8"
        );
        Ok(())
    }
}
