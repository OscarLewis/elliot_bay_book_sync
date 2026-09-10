use crate::{
    AppState,
    api::{
        get_uri_for_req::get_store_url_for_current_request, init_resources::ResourcesRoot,
        make_requests::make_request_to_kobo_store,
    },
};
use axum::http::{self, Uri};
use axum::{
    Json, extract,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use bytes::Bytes;
use serde_json::Value;
use tracing::{debug, error, info, warn};

pub(crate) async fn initialization_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
    uri: Uri,
    method: reqwest::Method,
    mut headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    debug!(
        token = token,
        proxy_kobo = state.config.proxy_kobo_store,
        "Handling Kobo initialization"
    );

    let mut kobo_resources: Option<ResourcesRoot> = None;
    let has_user_key = headers.contains_key("x-kobo-userkey");
    if state.config.proxy_kobo_store && !has_user_key {
        warn!("Attempting to proxy Kobo store without 'x-kobo-userkey' header")
    }

    if state.config.proxy_kobo_store {
        let store_url = get_store_url_for_current_request(&uri);
        // TODO Move client into app state
        let client = reqwest::Client::builder()
            .user_agent("Kobo Touch/4.38.21908 (Linux 2.6.35.3; U; en-US)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        if let Ok(store_response) =
            make_request_to_kobo_store(&client, method, &store_url, headers, body).await
        {
            let status = store_response.status();

            if let Ok(raw_bytes) = store_response.bytes().await {
                if raw_bytes.is_empty() {
                    warn!(status = %status, "Kobo store returned an EMPTY body (0 bytes). Check headers/auth.");
                } else {
                    match serde_json::from_slice::<Value>(&raw_bytes) {
                        Ok(json_val) => {
                            let error_code = json_val
                                .get("ResponseStatus")
                                .and_then(|rs| rs.get("ErrorCode"))
                                .and_then(|ec| ec.as_str());

                            if let Some(ec) = error_code {
                                if ec == "ExpiredToken" {
                                    info!(
                                        "Kobo Store session expired: ExpiredToken. Triggering re-authentication."
                                    );

                                    // Mirror cwa make_proxy_response function and pass upstream response straight through
                                    let mut proxy_headers = HeaderMap::new();
                                    // Copy non-connection-specific headers from upstream response if needed
                                    proxy_headers
                                        .insert("x-kobo-apitoken", "e30=".parse().unwrap());

                                    return (status, proxy_headers, raw_bytes).into_response();
                                }

                                warn!(
                                    status = %status,
                                    error_code = ec,
                                    "Kobo store returned error code '{ec}'. Using fallback."
                                );
                            } else if json_val.get("Resources").is_some() {
                                match serde_json::from_value::<ResourcesRoot>(json_val) {
                                    Ok(parsed_resources) => {
                                        kobo_resources = Some(parsed_resources);
                                    }
                                    Err(err) => {
                                        error!(
                                            error = %err,
                                            "Failed to deserialize valid Resources payload into ResourcesRoot"
                                        );
                                    }
                                }
                            } else {
                                warn!("Kobo response missing 'Resources' field. Using fallback.");
                            }
                        }
                        Err(err) => {
                            let body_snippet = String::from_utf8_lossy(&raw_bytes);
                            error!(
                                error = %err,
                                body = %body_snippet,
                                "Failed to parse response body from Kobo store as JSON."
                            );
                        }
                    }
                }
            } else {
                error!("Failed to read response bytes from Kobo store.");
            }
        } else {
            error!("Failed to send request to Kobo store.");
        }
    }

    // Fallback if proxying was disabled or failed to yield resources
    let resources = kobo_resources.unwrap_or_else(|| {
        debug!("Using fallback Kobo resource definitions");
        ResourcesRoot::default()
    });

    // TODO: Apply URL template/host rewrites to `resources` here (image_host, image_url_template, etc.)

    // Replicate Python response header: "x-kobo-apitoken" : "e30="
    // In Base64 encoding, an empty JSON object {} encodes directly to "e30=".
    let mut response_headers = HeaderMap::new();
    response_headers.insert("x-kobo-apitoken", "e30=".parse().unwrap());

    (StatusCode::OK, response_headers, Json(resources)).into_response()
}

#[cfg(test)]
mod tests {
    use crate::{AppState, config::AppConfig, test_helpers::setup_test_app};
    use axum::http::StatusCode;
    use serde_json::Value;
    use test_log::test;

    /// Verifies that calling GET `/v1/initialization/:token` returns a 200 OK
    /// and matches the JSON payload in `test_data/initialization.json`.
    // #[test(tokio::test)]
    // async fn test_initialization_handler_matches_cwa_test_data() {
    //     // Setup the app with proxying disabled to force stored defaults
    //     let mut config = AppConfig::default();
    //     config.proxy_kobo_store = false;
    //     let state = AppState::new(config);

    //     let server = setup_test_app(state);

    //     // Call the endpoint (using a mock token parameter)
    //     let response = server.get("/kobo/test-token-123/v1/initialization").await;
    //     response.assert_status(StatusCode::OK);

    //     // Load the expected JSON from disk
    //     let expected_json_str = std::fs::read_to_string("test_data/cwa_init.json")
    //         .expect("Failed to read test_data/cwa_init.json");
    //     let expected_json: Value = serde_json::from_str(&expected_json_str)
    //         .expect("Failed to parse test_data/cwa_init.json as JSON");

    //     // Extract the actual response body as a Value to ignore formatting/whitespace
    //     let actual_json: Value = response.json();

    //     // Compare the parsed JSON trees directly
    //     assert_eq!(actual_json, expected_json);
    // }

    #[test(tokio::test)]
    async fn test_initialization_handler_matches_kobo_test_data() {
        // Setup the app with proxying disabled to force stored defaults
        let mut config = AppConfig::default();
        config.proxy_kobo_store = false;
        let state = AppState::new(config);

        let server = setup_test_app(state);

        // Call the endpoint (using a mock token parameter)
        let response = server.get("/kobo/test-token-123/v1/initialization").await;
        response.assert_status(StatusCode::OK);

        // Load the expected JSON from disk
        let expected_json_str = std::fs::read_to_string("test_data/kobo_remote_init.json")
            .expect("Failed to read test_data/kobo_remote_init.json");
        let expected_json: Value = serde_json::from_str(&expected_json_str)
            .expect("Failed to parse test_data/kobo_remote_init.json as JSON");

        // Extract the actual response body as a Value to ignore formatting/whitespace
        let actual_json: Value = response.json();

        // Compare the parsed JSON trees directly
        assert_eq!(actual_json, expected_json);
    }
}
