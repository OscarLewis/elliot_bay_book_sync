use crate::{AppState, error::AppError};
use axum::{
    Json,
    extract::{self},
    http::Uri,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use serde::Serialize;
use tracing::{debug, info};

#[derive(Serialize, Debug)]
pub struct OidcConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub response_types_supported: Vec<&'static str>,
    pub grant_types_supported: Vec<&'static str>,
    pub subject_types_supported: Vec<&'static str>,
    pub id_token_signing_alg_values_supported: Vec<&'static str>,
    pub scopes_supported: Vec<&'static str>,
    pub token_endpoint_auth_methods_supported: Vec<&'static str>,
}

pub async fn oidc_well_known_configuration_handler(
    uri: Uri,
    extract::State(state): extract::State<AppState>,
) -> Result<Response, AppError> {
    // Recent Kobo firmware performs oidc endpoint discovery against oauth_host
    // before refreshing an expired access token.
    // When Kobo Store proxying is disabled, we host mock endpoints, so it must advertise them here.
    //
    // Without a token_endpoint the device requests "a new token from ''", raises a web request error and
    // cancels the whole sync queue (SyncLibraryCommand included), so
    // /v1/library/sync is never reached and the device reports "Sync failed".

    let patched_resources = state.patched_resources.lock().await;
    let oauth_url = patched_resources.oauth_host.clone();

    let config = OidcConfiguration {
        issuer: oauth_url.to_string(),
        authorization_endpoint: format!("{oauth_url}/auth"),
        token_endpoint: format!("{oauth_url}/token"),
        userinfo_endpoint: format!("{oauth_url}/userinfo"),
        jwks_uri: format!("{oauth_url}/jwks"),
        response_types_supported: vec!["code", "token"],
        grant_types_supported: vec!["authorization_code", "refresh_token"],
        subject_types_supported: vec!["public"],
        id_token_signing_alg_values_supported: vec!["RS256"],
        scopes_supported: vec!["openid", "offline_access"],
        token_endpoint_auth_methods_supported: vec![
            "client_secret_post",
            "client_secret_basic",
            "none",
        ],
    };

    info!(
        handler_uri = uri.to_string(),
        oauth_url,
        ?config,
        "Received oidc well known configuration request"
    );

    Ok(Json(config).into_response())
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::{MongoTestContext, setup_test_app};
    use reqwest::StatusCode;
    use test_context::test_context;
    use test_log::test;

    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    async fn test_oidc_endpoint(
        ctx: &mut MongoTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "https://books.example.com/";
        let token = "test-token-123";

        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.base_url = test_base_url.to_string();
        config.ebbooks_auth_key = token.to_string();
        config.proxy_kobo_store = false;
        state.config = std::sync::Arc::new(config);

        let server = setup_test_app(state);

        let response = server
            .get(&format!(
                "/kobo/{token}/oauth/.well-known/openid-configuration"
            ))
            .await;

        response.assert_status(StatusCode::OK);
        Ok(())
    }
}
