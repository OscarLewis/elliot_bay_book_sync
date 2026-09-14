use axum::{
    Json, Router,
    body::Bytes,
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::json;
use tracing::info;

use crate::{AppState, api::make_requests::redirect_or_proxy_request, error::AppError};

pub fn stub_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/user/loyalty/*path", any(dummy_proxy_handler))
        .route("/v1/user/profile", any(dummy_proxy_handler))
        .route("/v1/user/wishlist", any(dummy_proxy_handler))
        .route("/v1/user/recommendations", any(dummy_proxy_handler))
        .route("/v1/analytics/*path", any(dummy_proxy_handler))
        .route("/v1/assets", any(dummy_proxy_handler))
}

pub async fn dummy_proxy_handler(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    info!(
        method = %method,
        uri = %uri,
        proxy_kobo_store = state.config.proxy_kobo_store,
        "Unimplemented Kobo request received"
    );

    if state.config.proxy_kobo_store {
        let store_url = format!(
            "https://store.kobobooks.com{}",
            uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("")
        );

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

    // Return empty 200 OK or basic JSON payload to keep Nickel happy when not proxying
    Ok((StatusCode::OK, Json(json!({}))).into_response())
}
