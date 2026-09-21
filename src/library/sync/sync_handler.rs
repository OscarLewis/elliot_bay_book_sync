use crate::{
    AppState,
    api::make_requests::{get_download_url_format_for_book, make_request_to_kobo_store},
    error::AppError,
    library::{
        book::Book,
        sync::{
            entitlement_models::{Entitlement, SyncResult},
            sync_document::SyncedBookDocument,
            sync_token::SyncToken,
        },
    },
};
use axum::{
    extract::{self},
    http::{HeaderMap, HeaderValue, uri},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, TimeZone, Utc};
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

    debug!(
        origin_uri = %uri,
        ?headers,
        "Sync request received"
    );
    let mut sync_token = SyncToken::from_headers(&headers);
    debug!(?sync_token, "Sync token generated from headers");
    let url_format = get_download_url_format_for_book(&state.config.base_url, &token);
    debug!(url_format, "Download link format");

    // Fetch all synced book records and library books
    let synced_books = state.mongodb.syncs.fetch_all().await?;
    let books = state.mongodb.books.fetch_all().await?;

    /*
    TODO: Update to how we handle updated entitlements to allow for metadata refresh post intitial sync
    UNSYNCED
        ↓
    NewEntitlement

    SYNCED + modified
        ↓
    ChangedEntitlement
    */

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

    // Map book_id -> last_synced_at
    let synced_map: HashMap<&str, DateTime<Utc>> = synced_books
        .iter()
        .map(|sb| (sb.book_id.as_str(), sb.last_synced_at))
        .collect();

    // Find never-synced books (New Entitlements)
    let never_synced_books: Vec<&Book> = books
        .iter()
        .filter(|book| {
            book.id.as_ref().map_or(false, |id| {
                !synced_map.contains_key(id.to_string().as_str())
            })
        })
        .take(SYNC_ITEM_LIMIT)
        .collect();

    let remaining_limit = SYNC_ITEM_LIMIT.saturating_sub(never_synced_books.len());

    // Find previously synced books whose modified_at changed (Changed Entitlements)
    let modified_synced_books: Vec<&Book> = books
        .iter()
        .filter(|book| {
            book.id
                .as_ref()
                .and_then(|id| synced_map.get(id.to_string().as_str()))
                .map_or(false, |&last_synced_at| book.modified_at > last_synced_at)
        })
        .collect();

    debug!(
        num_never_synced = never_synced_books.len(),
        num_modified_synced = modified_synced_books.len(),
        "Found books to sync"
    );

    // Process Never-Synced Books -> NewEntitlement
    for book in never_synced_books {
        let book_id = book.id.ok_or(AppError::InvalidObjectId)?.to_string();
        has_local_books = true;

        let entitlement = Entitlement::from_book_tuple(
            (&book_id, book),
            state.config.clone(),
            None,
            None,
            None,
            false,
        );

        sync_results.push(SyncResult {
            changed_entitlement: None,
            new_entitlement: Some(entitlement),
            changed_reading_state: None,
            deleted_tag: None,
            new_tag: None,
            changed_tag: None,
        });

        debug!(book_id = %book_id, title = book.title, "Syncing new book");

        new_books_last_modified = std::cmp::max(new_books_last_modified, book.modified_at);
        new_books_last_created = std::cmp::max(new_books_last_created, book.modified_at);

        let mut synced = SyncedBookDocument {
            book_id,
            user_id: "default".to_string(),
            id: None,
            synced_at: Utc::now(),
            last_synced_at: Utc::now(),
        };

        state.mongodb.syncs.insert(&mut synced).await?;
    }

    // Process Modified Synced Books -> ChangedEntitlement
    for book in modified_synced_books {
        let book_id = book.id.ok_or(AppError::InvalidObjectId)?.to_string();
        has_local_books = true;

        let entitlement = Entitlement::from_book_tuple(
            (&book_id, book),
            state.config.clone(),
            None,
            None,
            None,
            false,
        );

        sync_results.push(SyncResult {
            changed_entitlement: Some(entitlement),
            new_entitlement: None,
            changed_reading_state: None,
            deleted_tag: None,
            new_tag: None,
            changed_tag: None,
        });

        debug!(book_id = %book_id, title = book.title, "Syncing modified book");

        new_books_last_modified = std::cmp::max(new_books_last_modified, book.modified_at);

        let mut synced = SyncedBookDocument {
            book_id,
            user_id: "default".to_string(),
            id: None,
            synced_at: Utc::now(),
            last_synced_at: Utc::now(),
        };

        state.mongodb.syncs.upsert(&mut synced).await?;
    }

    // Update sync token
    sync_token.data.books_last_modified = new_books_last_modified;
    sync_token.data.books_last_created = new_books_last_created;
    sync_token.data.archive_last_modified = new_archived_last_modified;
    sync_token.data.reading_state_last_modified = new_reading_state_last_modified;

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
            HeaderMap::new(),
            serde_json::to_vec(sync_token)?.into(),
            Some(sync_token),
        )
        .await
        {
            Ok(store_response) => {
                // Copy headers from store response regardless of body shape
                for name in ["x-kobo-sync", "x-kobo-sync-mode", "x-kobo-recent-reads"] {
                    if let Some(value) = store_response.headers().get(name) {
                        response_headers.insert(name, value.clone());
                    }
                }

                // Merge store response token
                sync_token.merge_from_store_response(store_response.headers());

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
            }
            Err(e) => {
                error!(?e, "Failed to receive response from Kobo's sync endpoint");
            }
        }
    }

    // Add continuation header if needed
    if set_cont {
        response_headers.insert("x-kobo-sync", HeaderValue::from_static("continue"));
    }

    // Add sync token to response headers
    sync_token.to_headers(&mut response_headers);

    debug!(books_synced = final_results.len(), "Kobo sync completed");

    // Build JSON response with proper encoding
    let json_body = serde_json::to_string(&final_results)?;
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );

    Ok((response_headers, json_body).into_response())
}

#[cfg(test)]
mod tests {
    use crate::{
        library::sync::{
            sync_handler::{SYNC_ITEM_LIMIT, generate_sync_response},
            sync_token::{SYNC_TOKEN_HEADER, SyncToken},
        },
        metadata::update_meta::update_metadata,
        test_helpers::{AppTestContext, setup_test_app},
    };
    use axum::http::HeaderMap;
    use chrono::{TimeZone, Utc};
    use reqwest::StatusCode;
    use test_context::test_context;
    use test_log::test;
    use tokio::time::{Duration, sleep};

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_library_sync_status_code(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_no_proxy(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&HeaderMap::new());
        let sync_results = vec![];

        let response =
            generate_sync_response(&mut sync_token, sync_results, false, &state, false).await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_with_continuation(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&HeaderMap::new());
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

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_includes_token(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let mut sync_token = SyncToken::from_headers(&HeaderMap::new());
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

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_library_sync_updates_and_returns_valid_sync_token(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        let expected_book_count = books_needing_metadata.len();
        update_metadata(state.clone(), books_needing_metadata).await?;

        // Wait for metadata update and image extraction to complete
        sleep(Duration::from_millis(2500)).await;

        // Initial Sync Call (No Sync-Token in Header)
        let response = server.get(&format!("/kobo/{token}/v1/library/sync")).await;
        response.assert_status(StatusCode::OK);

        // Assert header presence
        let token_header = response
            .headers()
            .get(SYNC_TOKEN_HEADER)
            .expect("x-kobo-synctoken header missing from response")
            .to_str()?;

        // Reconstruct and verify SyncToken payload
        let parsed_token = SyncToken::from_headers(response.headers());

        // Verify token state reflects latest book additions/modifications
        assert_ne!(
            parsed_token.data.books_last_modified,
            Utc.timestamp_opt(0, 0).unwrap(),
            "Expected books_last_modified in token to be updated past epoch"
        );
        assert_ne!(
            parsed_token.data.books_last_created,
            Utc.timestamp_opt(0, 0).unwrap(),
            "Expected books_last_created in token to be updated past epoch"
        );

        // Verify JSON body payload contains synced entitlement items
        let body_bytes = response.as_bytes();
        let sync_results: Vec<serde_json::Value> = serde_json::from_slice(body_bytes)?;
        assert_eq!(
            sync_results.len(),
            std::cmp::min(expected_book_count, SYNC_ITEM_LIMIT),
            "Returned sync payload item count mismatch"
        );

        // Second Sync Call using the newly received token
        let re_sync_response = server
            .get(&format!("/kobo/{token}/v1/library/sync"))
            .add_header(SYNC_TOKEN_HEADER, token_header)
            .await;

        re_sync_response.assert_status(StatusCode::OK);

        // Verify second sync returns empty entitlements as no new books were added
        let re_sync_body = re_sync_response.into_bytes();
        let re_sync_results: Vec<serde_json::Value> = serde_json::from_slice(&re_sync_body)?;
        assert!(
            re_sync_results.is_empty(),
            "Expected no new synced entitlements on second call with existing token"
        );

        // Cleanup: remove all downloaded images
        let all_books = state.mongodb.books.fetch_all().await?;
        for book in all_books {
            if let Some(image_path) = book.image_path {
                let _ = tokio::fs::remove_file(&image_path).await;
            }
        }

        Ok(())
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_generate_sync_response_encodes_token_correctly(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = "http://books.example.com/".to_string();
        config.ebbooks_auth_key = "test-token".to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        // Explicitly set custom timestamps on token
        let mut sync_token = SyncToken::from_headers(&HeaderMap::new());
        let test_timestamp = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        sync_token.data.books_last_modified = test_timestamp;
        sync_token.data.books_last_created = test_timestamp;

        let response =
            generate_sync_response(&mut sync_token, vec![], false, &state, false).await?;

        // Token header should be present
        assert!(
            response.headers().contains_key(SYNC_TOKEN_HEADER),
            "Token header should be present"
        );

        // Decode token back to verify value propagation
        let reconstructed_token = SyncToken::from_headers(response.headers());

        assert_eq!(
            reconstructed_token.data.books_last_modified, test_timestamp,
            "books_last_modified was not preserved through response header serialization"
        );
        assert_eq!(
            reconstructed_token.data.books_last_created, test_timestamp,
            "books_last_created was not preserved through response header serialization"
        );

        Ok(())
    }

    // TODO write a test that edits the metadata and modified_at for a book post sync and confirms that a ChangedEntitlement is generated
}
