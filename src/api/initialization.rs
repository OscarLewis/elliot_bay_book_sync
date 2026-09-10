use crate::{AppState, api::init_resources::ResourcesRoot};
use axum::{Json, extract, http::StatusCode, response};
use tracing::debug;

pub(crate) async fn initialization_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
) -> impl response::IntoResponse {
    debug!(
        token = token,
        proxy_kobo = state.config.proxy_kobo_store,
        "Handling Kobo initialization"
    );

    let resources = if state.config.proxy_kobo_store {
        debug!("Attempting to proxy kobo store");
        // TODO: Replace this with proxied logic
        ResourcesRoot::default()
    } else {
        debug!("Skipping kobo store proxy, using stored values");
        ResourcesRoot::default()
    };

    // Serializes `ResourcesRoot` directly into the JSON response
    (StatusCode::OK, Json(resources))
}

#[cfg(test)]
mod tests {
    use crate::{AppState, config::AppConfig, test_helpers::setup_test_app};
    use axum::http::StatusCode;
    use serde_json::Value;
    use test_log::test;

    /// Verifies that calling GET `/v1/initialization/:token` returns a 200 OK
    /// and matches the JSON payload in `test_data/initialization.json`.
    #[test(tokio::test)]
    async fn test_initialization_handler_matches_test_data() {
        // Setup the app with proxying disabled to force stored defaults
        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;
        let state = AppState::new(config);

        let server = setup_test_app(state);

        // Call the endpoint (using a mock token parameter)
        let response = server.get("/kobo/test-token-123/v1/initialization").await;
        response.assert_status(StatusCode::OK);

        // Load the expected JSON from disk
        let expected_json_str = std::fs::read_to_string("test_data/initialization.json")
            .expect("Failed to read test_data/initialization.json");
        let expected_json: Value = serde_json::from_str(&expected_json_str)
            .expect("Failed to parse test_data/initialization.json as JSON");

        // Extract the actual response body as a Value to ignore formatting/whitespace
        let actual_json: Value = response.json();

        // Compare the parsed JSON trees directly
        assert_eq!(actual_json, expected_json);
    }
}
