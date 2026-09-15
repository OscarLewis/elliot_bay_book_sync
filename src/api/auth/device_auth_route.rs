use crate::{
    AppState,
    api::make_requests::{get_store_url_for_current_request, redirect_or_proxy_request},
    error::AppError,
};
use axum::{
    Json,
    body::Bytes,
    extract::{self, OriginalUri},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rand::Rng;
use reqwest::Method;
use tracing::{debug, info};

pub async fn device_auth_request_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    info!(
        uri = uri.to_string(),
        token,
        proxy_kobo_store = state.config.proxy_kobo_store,
        "Received auth request"
    );

    if state.config.proxy_kobo_store {
        let store_url = get_store_url_for_current_request(state.clone(), &uri, &token).await?;
        return Ok(redirect_or_proxy_request(
            &state.req_client,
            state.config.proxy_kobo_store,
            Method::POST,
            &store_url,
            headers,
            body,
            None,
        )
        .await
        .into_response());
    }
    Ok(make_mock_auth_response(body).into_response())
}

/// Generates a dummy authentication response for ebbooks.
///
/// We do not make practical use of the actual auth/device API for actual authentication.
/// This function returns a dummy response with random tokens just to keep the device happy.
///
/// # Arguments
/// * `body` - The incoming request body containing the UserKey
///
/// # Returns
/// A JSON response with dummy access/refresh tokens
fn make_mock_auth_response(body: Bytes) -> impl IntoResponse {
    // Parse the incoming request to extract UserKey
    let user_key = if let Ok(content) = serde_json::from_slice::<serde_json::Value>(&body) {
        content
            .get("UserKey")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };

    // Generate random access and refresh tokens
    let mut rng = rand::rng();

    let mut access_bytes = [0u8; 24];
    let mut refresh_bytes = [0u8; 24];

    rng.fill_bytes(&mut access_bytes);
    rng.fill_bytes(&mut refresh_bytes);

    let access_token = STANDARD.encode(access_bytes);
    let refresh_token = STANDARD.encode(refresh_bytes);
    let response_body = serde_json::json!({
        "AccessToken": access_token,
        "RefreshToken": refresh_token,
        "TokenType": "Bearer",
        "TrackingId": uuid::Uuid::new_v4().to_string(),
        "UserKey": user_key,
    });
    debug!(
        auth_body = ?response_body,
        "Generated mock authentication response"
    );
    Json(response_body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::init_resources::Resources, config::AppConfig, database::document::DocumentDB,
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::sync::Arc;
    use test_log::test;
    use tokio::sync::Mutex;

    #[test(tokio::test)]
    async fn test_auth_handler() -> Result<(), Box<dyn std::error::Error>> {
        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;
        let db = DocumentDB::open_in_memory()?;

        let state = AppState {
            config: Arc::new(config),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            kobo_resources: Arc::new(Mutex::new(Resources::default())),
            hardcover_api_token: None,
            patched_resources: Arc::new(Mutex::new(Resources::default())),
        };

        let server = setup_test_app(state);

        let token = "test-token-123";

        let response = server.post(&format!("/kobo/{token}/v1/auth/device")).await;

        // Assert response is successful
        assert_eq!(response.status_code(), StatusCode::OK);

        // Parse response body as JSON
        let body = response.json::<serde_json::Value>();

        // Assert required fields are present
        assert!(body.get("AccessToken").is_some());
        assert!(body.get("RefreshToken").is_some());
        assert_eq!(
            body.get("TokenType").and_then(|v| v.as_str()),
            Some("Bearer")
        );
        assert!(body.get("TrackingId").is_some());
        assert!(body.get("UserKey").is_some());

        // Assert tokens are non-empty base64 strings
        let access_token = body.get("AccessToken").and_then(|v| v.as_str());
        let refresh_token = body.get("RefreshToken").and_then(|v| v.as_str());
        assert!(access_token.is_some_and(|t| !t.is_empty()));
        assert!(refresh_token.is_some_and(|t| !t.is_empty()));

        // Assert TrackingId is a valid UUID
        let tracking_id = body.get("TrackingId").and_then(|v| v.as_str());
        assert!(tracking_id.is_some_and(|id| uuid::Uuid::parse_str(id).is_ok()));

        Ok(())
    }
}
