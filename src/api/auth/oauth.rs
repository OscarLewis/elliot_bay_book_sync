use crate::{AppState, error::AppError};
use axum::{
    Json, extract,
    http::Uri,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::Rng;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// Request payload extractor
#[derive(Debug, Deserialize, Default)]
pub struct OauthRequestPayload {
    pub scope: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OauthPathParams {
    // Authentication token
    pub token: String,
    // Subpath after "/kobo/{token}/oauth/"
    pub subpath: String,
}
// OAuth Response structure matching Calibre-Web fields
#[derive(Debug, Serialize)]
pub struct OauthResponsePayload {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: u64,
    pub scope: String,
    pub user_id: String,
    // Legacy field names used by Kobo devices
    #[serde(rename = "AccessToken")]
    pub access_token_legacy: String,
    #[serde(rename = "RefreshToken")]
    pub refresh_token_legacy: String,
    #[serde(rename = "TokenType")]
    pub token_type_legacy: &'static str,
}

// Generates a random 24-byte base64 string
fn generate_random_oauth_token() -> String {
    let mut rng = rand::rng();
    let mut random_bytes = [0u8; 24];
    rng.fill_bytes(&mut random_bytes);
    STANDARD.encode(random_bytes)
}

// Handler: "/kobo/{token}/oauth/token"
pub async fn oauth_token_handler(
    extract::Path(params): extract::Path<OauthPathParams>,
    uri: Uri,
    extract::State(_state): extract::State<AppState>,
    payload: Option<Json<OauthRequestPayload>>,
) -> Result<Response, AppError> {
    // params extracts both the token and wildcard subpath
    info!(uri = uri.to_string(), ?params, "Received oauth request");

    let payload = payload.map(|Json(p)| p).unwrap_or_default();

    let access_token = generate_random_oauth_token();
    let refresh_token = generate_random_oauth_token();

    let response_body = OauthResponsePayload {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        token_type: "Bearer",
        expires_in: 3600,
        scope: payload.scope.unwrap_or_default(),
        user_id: payload.user_id.unwrap_or_default(),
        access_token_legacy: access_token,
        refresh_token_legacy: refresh_token,
        token_type_legacy: "Bearer",
    };

    Ok((StatusCode::OK, Json(response_body)).into_response())
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
    async fn test_oauth_token_handler() -> Result<(), Box<dyn std::error::Error>> {
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

        // POST request with a payload
        let payload = serde_json::json!({
            "scope": "read_write",
            "user_id": "user_123"
        });

        let response = server
            .post(&format!("/kobo/{token}/oauth/token"))
            .json(&payload)
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body = response.json::<serde_json::Value>();

        // Assert camelCase and PascalCase (legacy) fields are present and correct
        assert_eq!(
            body.get("token_type").and_then(|v| v.as_str()),
            Some("Bearer")
        );
        assert_eq!(
            body.get("TokenType").and_then(|v| v.as_str()),
            Some("Bearer")
        );
        assert_eq!(body.get("expires_in").and_then(|v| v.as_u64()), Some(3600));
        assert_eq!(
            body.get("scope").and_then(|v| v.as_str()),
            Some("read_write")
        );
        assert_eq!(
            body.get("user_id").and_then(|v| v.as_str()),
            Some("user_123")
        );

        // Assert token values exist, match each other, and are non-empty
        let access_token = body.get("access_token").and_then(|v| v.as_str());
        let legacy_access_token = body.get("AccessToken").and_then(|v| v.as_str());
        assert!(access_token.is_some_and(|t| !t.is_empty()));
        assert_eq!(access_token, legacy_access_token);

        let refresh_token = body.get("refresh_token").and_then(|v| v.as_str());
        let legacy_refresh_token = body.get("RefreshToken").and_then(|v| v.as_str());
        assert!(refresh_token.is_some_and(|t| !t.is_empty()));
        assert_eq!(refresh_token, legacy_refresh_token);

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_oauth_token_handler_empty_payload() -> Result<(), Box<dyn std::error::Error>> {
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

        // POST request without body (simulates request.get_json(silent=True) or {})
        let response = server.post(&format!("/kobo/{token}/oauth/token")).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        let body = response.json::<serde_json::Value>();

        // Missing fields should safely default to empty strings
        assert_eq!(body.get("scope").and_then(|v| v.as_str()), Some(""));
        assert_eq!(body.get("user_id").and_then(|v| v.as_str()), Some(""));

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_oauth_token_handler_other_subpath() -> Result<(), Box<dyn std::error::Error>> {
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
        let response = server.post(&format!("/kobo/{token}/oauth/refresh")).await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.json::<serde_json::Value>();
        assert_eq!(
            body.get("token_type").and_then(|v| v.as_str()),
            Some("Bearer")
        );
        assert_eq!(
            body.get("TokenType").and_then(|v| v.as_str()),
            Some("Bearer")
        );
        assert_eq!(body.get("expires_in").and_then(|v| v.as_u64()), Some(3600));
        let access_token = body.get("access_token").and_then(|v| v.as_str());
        assert!(access_token.is_some_and(|t| !t.is_empty()));
        let refresh_token = body.get("refresh_token").and_then(|v| v.as_str());
        assert!(refresh_token.is_some_and(|t| !t.is_empty()));
        Ok(())
    }
}
