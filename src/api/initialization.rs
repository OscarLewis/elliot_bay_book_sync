use crate::{
    AppState,
    api::{
        init_resources::{ResourcesRoot, patch_kobo_resources},
        make_requests::{get_store_url_for_current_request, make_request_to_kobo_store},
    },
    error::AppError,
    resolve_base_url,
};
use axum::http::Uri;
use axum::{
    Json, extract,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use bytes::Bytes;
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// Handles a Kobo initialization request
///
/// Optionally proxies the request to the Kobo store to obtain user-specific resources
/// Falls back to the default resources when the store cannot provide usable resources
/// Patches resource URLs to point back to this server before returning the response
pub(crate) async fn initialization_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
    uri: Uri,
    method: reqwest::Method,
    headers: HeaderMap,
    body: Bytes,
) -> Result<axum::response::Response, AppError> {
    debug!(
        token = %token,
        proxy_kobo = state.config.proxy_kobo_store,
        "Handling Kobo initialization request"
    );

    // Resources returned by the Kobo store, when available
    let mut kobo_resources: Option<ResourcesRoot> = None;

    if state.config.proxy_kobo_store {
        // Kobo may not return user-specific resources without the user key
        if !headers.contains_key("x-kobo-userkey") {
            warn!(
                "Proxying Kobo store without 'x-kobo-userkey' header; upstream response may lack user resources"
            );
        }

        // Build the upstream Kobo store URL from the current request
        let store_url = get_store_url_for_current_request(state.clone(), &uri, &token).await?;

        // Proxy the initialization request to the Kobo store
        match make_request_to_kobo_store(
            &state.req_client,
            method,
            &store_url,
            headers.clone(),
            body,
        )
        .await
        {
            Ok(store_response) => {
                // Process the upstream response and handle responses that must be returned directly
                if let Some(early_response) =
                    process_store_response(store_response, &mut kobo_resources).await
                {
                    return Ok(early_response);
                }
            }
            Err(err) => {
                // Continue with the local fallback when the Kobo store request fails
                error!(error = %err, "Failed to proxy request to Kobo store API");
            }
        }
    }

    // Fall back to the default resources when proxying is disabled or did not produce usable resources
    let mut resources = kobo_resources.unwrap_or_else(|| {
        debug!("Using fallback default Kobo resource definitions");
        ResourcesRoot::default()
    });

    // Resolve the base URL used when rewriting resource endpoints
    let base_url = resolve_base_url(&headers, None);

    // Store canonical resources in state for later proxy requests
    let mut kobo_resources = state.kobo_resources.lock().await;
    *kobo_resources = resources.resources.clone();

    debug!("Stored canonical kobo resources");

    let patched_resources = patch_kobo_resources(
        resources.resources,
        &base_url,
        &token,
        state.config.proxy_kobo_store,
    );

    let mut patched_resources_state = state.patched_resources.lock().await;
    *patched_resources_state = patched_resources.clone();

    debug!("Stored patched kobo resources");

    // Return the resources with the API token expected by Kobo (it's empty lol)
    let mut response_headers = HeaderMap::new();
    response_headers.insert("x-kobo-apitoken", "e30=".parse().unwrap());

    Ok((
        StatusCode::OK,
        response_headers,
        Json(ResourcesRoot {
            resources: patched_resources,
        }),
    )
        .into_response())
}

/// Parses the Kobo upstream store response
///
/// Returns `Some(Response)` when the upstream response must be returned directly to the device
/// Otherwise, populates `kobo_resources` when the response contains valid initialization resources
async fn process_store_response(
    response: reqwest::Response,
    kobo_resources: &mut Option<ResourcesRoot>,
) -> Option<axum::response::Response> {
    // Preserve the upstream status in case the response needs to be returned directly
    let status = response.status();

    // Read the complete upstream response body before parsing it
    let raw_bytes = match response.bytes().await {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => {
            warn!(%status, "Kobo store returned an empty response body (0 bytes)");
            return None;
        }
        Err(err) => {
            error!(error = %err, "Failed to read response bytes from Kobo store");
            return None;
        }
    };

    // Parse JSON payload
    let json_val: Value = match serde_json::from_slice(&raw_bytes) {
        Ok(json) => json,
        Err(err) => {
            // Include the upstream body when reporting malformed JSON
            let body_snippet = String::from_utf8_lossy(&raw_bytes);
            error!(
                error = %err,
                body = %body_snippet,
                "Failed to parse upstream response body from Kobo store as JSON"
            );
            return None;
        }
    };

    // Check for explicit Kobo API error codes
    if let Some(ec) = json_val
        .get("ResponseStatus")
        .and_then(|rs| rs.get("ErrorCode"))
        .and_then(|ec| ec.as_str())
    {
        if ec == "ExpiredToken" {
            // Kobo uses this response to determine that authentication is required again
            info!("Kobo Store session expired: ExpiredToken. Triggering re-authentication.");
            let mut proxy_headers = HeaderMap::new();
            proxy_headers.insert("x-kobo-apitoken", "e30=".parse().unwrap());
            return Some((status, proxy_headers, raw_bytes).into_response());
        }

        // Other upstream errors are handled by falling back to the default resources
        warn!(
            %status,
            error_code = ec,
            "Kobo store returned error status code '{ec}'. Falling back to default resources."
        );
        return None;
    }

    // Deserialize valid resource payload
    if json_val.get("Resources").is_some() {
        match serde_json::from_value::<ResourcesRoot>(json_val) {
            Ok(parsed) => {
                // Keep the parsed resources so the caller can patch their URLs
                info!("Parsed initialization resources from Kobo store");
                *kobo_resources = Some(parsed);
            }
            Err(err) => {
                error!(error = %err, "Failed to deserialize 'Resources' payload from Kobo store");
            }
        }
    } else {
        // Responses without Resources cannot be used as the initialization payload
        let body_snippet = String::from_utf8_lossy(&raw_bytes);
        warn!(
            %status,
            body = %body_snippet,
            "Kobo store response missing 'Resources' field (likely missing x-kobo-userkey header). Using fallback."
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState,
        api::init_resources::{ResourcesRoot, patch_kobo_resources},
        config::AppConfig,
        database::document::DocumentDB,
        test_helpers::setup_test_app,
    };
    use axum::http::StatusCode;
    use test_log::test;

    /// Verifies that the initialization handler matches the known Kobo test data
    #[test(tokio::test)]
    async fn test_initialization_handler_matches_kobo_test_data() {
        // Disable store proxying so the test exercises the local fallback resources
        let mut config = AppConfig::default();
        let db = DocumentDB::open(&config.database_path).expect("Unable to open database");
        config.proxy_kobo_store = false;

        let state = AppState::new(config, db, None);

        // Start the test application with the configured state
        let server = setup_test_app(state.clone());

        let token = "test-token-123";
        let response = server
            .get(&format!("/kobo/{token}/v1/initialization"))
            .await;
        response.assert_status(StatusCode::OK);

        // Load the expected initialization resources from the Kobo test data
        let expected_json_str = std::fs::read_to_string("test_data/kobo_remote_init.json")
            .expect("Failed to read test_data/kobo_remote_init.json");

        // Deserialize the expected resources so they can be patched in the same way as the response
        let expected_resources: ResourcesRoot = serde_json::from_str(&expected_json_str)
            .expect("Failed to parse test_data/kobo_remote_init.json as ResourcesRoot");

        let patched_resources = patch_kobo_resources(
            expected_resources.resources,
            "http://localhost",
            token,
            state.config.proxy_kobo_store,
        );

        let actual_root: ResourcesRoot = response.json();
        let actual_resources = actual_root.resources;

        assert_eq!(actual_resources, patched_resources);
    }
}
