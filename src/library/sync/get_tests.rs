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
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use reqwest::Method;
use serde_json::json;
use tracing::{debug, info};

pub async fn get_tests_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
    uri: Uri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    if state.config.proxy_kobo_store {
        let store_url = get_store_url_for_current_request(state.clone(), &uri, &token).await?;

        return Ok(redirect_or_proxy_request(
            &state.req_client,
            state.config.proxy_kobo_store,
            method,
            &store_url,
            headers,
            body,
            None,
        )
        .await
        .into_response());
    }

    let test_key = headers
        .get("X-Kobo-userkey")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    Ok(Json(json!({
        "Result": "Success",
        "TestKey": test_key,
        "Tests": {}
    }))
    .into_response())
}
