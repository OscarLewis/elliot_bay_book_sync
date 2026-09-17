//! Handle routing for `/kobo/{token}/v1/library/{book_uuid}/state`
//! This is either a PUT request to update our database with the latest Reading State
//! or a GET request
//! of a book.

use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    database::document::DocumentTable,
    error::AppError,
    library::{
        book::{
            Book, CurrentBookmarkDocument, ReadStatus, ReadingStateDocument, StatisticsDocument,
            StatusInfoDocument,
        },
        sync::entitlement_models::ReadingState,
    },
};
use axum::{
    Json,
    body::Bytes,
    extract::{self, OriginalUri},
    http::{HeaderMap, Method},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct KoboSyncPayload {
    pub reading_states: Vec<ReadingStatePayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ReadingStatePayload {
    pub entitlement_id: String,
    pub current_bookmark: Option<BookmarkPayload>,
    pub statistics: Option<StatisticsPayload>,
    pub status_info: Option<StatusInfoPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BookmarkPayload {
    pub progress_percent: f64,
    pub content_source_progress_percent: f64,
    pub location: Option<LocationPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LocationPayload {
    pub value: String,
    pub r#type: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StatisticsPayload {
    pub spent_reading_minutes: i32,
    pub remaining_time_minutes: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StatusInfoPayload {
    pub status: String,
}

impl StatisticsPayload {
    pub fn into_document(self, now: DateTime<Utc>) -> StatisticsDocument {
        StatisticsDocument {
            spent_reading_minutes: Some(self.spent_reading_minutes),
            remaining_time_minutes: Some(self.remaining_time_minutes),
            last_modified: now,
        }
    }
}
impl BookmarkPayload {
    pub fn into_document(self, now: DateTime<Utc>) -> CurrentBookmarkDocument {
        let (location_value, location_type, location_source) = match self.location {
            Some(loc) => (Some(loc.value), Some(loc.r#type), Some(loc.source)),
            None => (None, None, None),
        };

        CurrentBookmarkDocument {
            progress_percent: Some(self.progress_percent),
            content_source_progress_percent: Some(self.content_source_progress_percent),
            location_value,
            location_type,
            location_source,
            last_modified: now,
        }
    }
}

impl StatusInfoPayload {
    pub fn into_document(self, now: DateTime<Utc>) -> StatusInfoDocument {
        StatusInfoDocument {
            // Map the Kobo string status ("Reading", "Unread", "Finished") to your ReadStatus enum
            status: match self.status.as_str() {
                "Reading" | "InProgress" => ReadStatus::InProgress,
                "Finished" | "Read" => ReadStatus::Finished,
                _ => ReadStatus::Unread,
            },
            last_modified: now,
            last_time_started_reading: None,
            times_started_reading: 0,
        }
    }
}

pub async fn reading_state_handler(
    extract::Path((token, book_id)): extract::Path<(String, String)>,
    extract::State(state): extract::State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    // ReadingState is the Entitlement model for reading state (what gets sent to the actual device)
    // ReadingStateDocument is the database model for reading state that we keep stored as part of the "Books" document
    // This translates between the two
    info!(
        uri = uri.to_string(),
        book_id, "Received Kobo ReadingState request"
    );
    let book_id = ObjectId::parse_str(&book_id).map_err(|_| AppError::InvalidObjectId)?;
    // let book: Option<Book> = state.db.read(DocumentTable::Books, &book_id)?;
    let book = state.mongodb.books.find_by_id(book_id).await?;
    let Some(mut book) = book else {
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

    // Book is found in database - match it's reading state
    match book.reading_state {
        Some(doc) => {
            // State exists -> map document to API response type
            let response = ReadingState::from_document(&doc);
            return Ok(Json(response).into_response());
        }
        None => match method {
            Method::GET => {
                // No state exists -> create default document and convert to API response
                let now = Utc::now();
                let new_doc = ReadingStateDocument {
                    book_id: book_id.clone(),
                    entitlement_id: book_id.to_string().clone(),
                    created: now,
                    last_modified: now,
                    priority_timestamp: now,
                    status_info: StatusInfoDocument {
                        status: ReadStatus::Unread,
                        last_modified: now,
                        last_time_started_reading: None,
                        times_started_reading: 0,
                    },
                    statistics: None,
                    current_bookmark: None,
                };

                // Store in book before DB update
                book.reading_state = Some(new_doc.clone());

                state.mongodb.books.update(book_id, &book).await?;

                // state.db.update(
                //     DocumentTable::Books,
                //     &book_id,
                //     &book,
                //     Some(|book: &Book| book.path.to_str().unwrap()),
                //     None,
                // )?;

                let response = ReadingState::from_document(&new_doc);
                return Ok(Json(response).into_response());
            }
            Method::PUT => {
                let payload: KoboSyncPayload = serde_json::from_slice(&body)
                    .map_err(|e| AppError::BadRequest(format!("Invalid JSON body: {e}")))?;

                // Take the first reading state from the request payload
                let reading_state_req =
                    payload.reading_states.into_iter().next().ok_or_else(|| {
                        AppError::BadRequest("Missing ReadingStates element".into())
                    })?;

                let now = Utc::now();
                let new_doc = ReadingStateDocument {
                    book_id: book_id.clone(),
                    entitlement_id: book_id.to_string().clone(),
                    created: now,
                    last_modified: now,
                    priority_timestamp: now,
                    status_info: reading_state_req
                        .status_info
                        .map(|s| s.into_document(now))
                        .unwrap_or_else(|| StatusInfoDocument {
                            status: ReadStatus::Unread,
                            last_modified: now,
                            last_time_started_reading: None,
                            times_started_reading: 0,
                        }),
                    statistics: reading_state_req.statistics.map(|s| s.into_document(now)),
                    current_bookmark: reading_state_req
                        .current_bookmark
                        .map(|b| b.into_document(now)),
                };

                // Store in book before DB update
                book.reading_state = Some(new_doc.clone());
                state.mongodb.books.update(book_id, &book).await?;

                // state.db.update(
                //     DocumentTable::Books,
                //     &book_id,
                //     &book,
                //     Some(|book: &Book| book.path.to_str().unwrap()),
                //     None,
                // )?;

                // Return success response
                let update_response = serde_json::json!({
                    "RequestResult": "Success",
                    "UpdateResults": [{
                        "EntitlementId": book_id.to_string(),
                        "CurrentBookmarkResult": { "Result": "Success" },
                        "StatisticsResult": { "Result": "Success" },
                        "StatusInfoResult": { "Result": "Success" }
                    }]
                });

                return Ok((StatusCode::OK, Json(update_response)).into_response());
            }
            // _ => Err(AppError::MethodNotAllowed),
            _ => {}
        },
    }
    Ok(StatusCode::OK.into_response())
}

#[cfg(test)]
mod tests {
    use crate::{
        library::{
            book::{Book, ReadStatus},
            sync::entitlement_models::ReadingState,
        },
        test_helpers::{MongoTestContext, setup_test_app},
    };
    use axum::http::StatusCode;
    use test_context::test_context;
    use test_log::test;

    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    async fn test_get_method_store_reading_state(
        ctx: &mut MongoTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = ctx.state.clone();
        let server = setup_test_app(state.clone());

        let epub_path = std::path::PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        let mut book = Book::from_path(epub_path);
        let book_id = state.mongodb.books.insert(&mut book).await?;

        let token = state.config.ebbooks_auth_key.clone();
        let response = server
            .get(&format!("/kobo/{token}/v1/library/{book_id}/state"))
            .await;
        response.assert_status(StatusCode::OK);

        let response_body: ReadingState = response.json();
        assert_eq!(response_body.entitlement_id, book_id.to_string());

        let updated_book = state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        let db_reading_state = updated_book
            .reading_state
            .expect("Reading state should be initialized on the book");

        assert_eq!(db_reading_state.book_id, book_id);
        assert_eq!(db_reading_state.entitlement_id, book_id.to_string());

        let second_get_response = server
            .get(&format!("/kobo/{token}/v1/library/{book_id}/state"))
            .await;
        second_get_response.assert_status(StatusCode::OK);

        let fetched_state: ReadingState = second_get_response.json();
        assert_eq!(fetched_state.entitlement_id, book_id.to_string());

        Ok(())
    }

    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    async fn test_put_method_updates_reading_state(
        ctx: &mut MongoTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = ctx.state.clone();
        let server = setup_test_app(state.clone());

        let epub_path = std::path::PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        let mut book = Book::from_path(epub_path);
        let book_id = state.mongodb.books.insert(&mut book).await?;

        let token = state.config.ebbooks_auth_key.clone();
        let put_payload = serde_json::json!({
            "ReadingStates": [{
                "EntitlementId": book_id.to_string(),
                "StatusInfo": {
                    "Status": "Reading",
                    "TimesStartedReading": 1
                },
                "Statistics": {
                    "SpentReadingMinutes": 12,
                    "RemainingTimeMinutes": 48
                },
                "CurrentBookmark": {
                    "ProgressPercent": 25,
                    "ContentSourceProgressPercent": 25,
                    "Location": {
                        "Value": "chapter-3",
                        "Type": "KoboSpan",
                        "Source": "epub"
                    }
                }
            }]
        });

        let response = server
            .put(&format!("/kobo/{token}/v1/library/{book_id}/state"))
            .json(&put_payload)
            .await;
        response.assert_status(StatusCode::OK);

        let response_body: serde_json::Value = response.json();
        assert_eq!(
            response_body["UpdateResults"][0]["EntitlementId"],
            book_id.to_string()
        );

        let updated_book = state
            .mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("Book should exist in database");

        let db_reading_state = updated_book
            .reading_state
            .expect("Reading state should exist on the book");

        assert_eq!(db_reading_state.book_id.to_hex(), book_id.to_hex());
        assert_eq!(db_reading_state.entitlement_id, book_id.to_string());
        assert_eq!(db_reading_state.status_info.status, ReadStatus::InProgress);
        assert!(db_reading_state.statistics.is_some());
        assert!(db_reading_state.current_bookmark.is_some());

        Ok(())
    }

    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    async fn test_put_method_missing_reading_states_returns_bad_request(
        ctx: &mut MongoTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = ctx.state.clone();
        let server = setup_test_app(state.clone());

        let epub_path = std::path::PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );
        let mut book = Book::from_path(epub_path);
        let book_id = state.mongodb.books.insert(&mut book).await?;

        let token = state.config.ebbooks_auth_key.clone();
        let put_payload = serde_json::json!({
            "ReadingStates": []
        });

        let response = server
            .put(&format!("/kobo/{token}/v1/library/{book_id}/state"))
            .json(&put_payload)
            .await;

        response.assert_status(StatusCode::BAD_REQUEST);

        Ok(())
    }
}
