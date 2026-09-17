use std::{collections::HashMap, str::FromStr};

use axum::{
    Json,
    body::Bytes,
    http::{HeaderMap, Method, Uri},
    response::{IntoResponse, Redirect},
};
use reqwest::{Client, Response as ReqwestResponse, StatusCode};
use tracing::debug;
use url::Url;

use crate::{
    AppState,
    error::AppError,
    library::sync::sync_token::{SYNC_TOKEN_HEADER, SyncToken},
};

/// Headers that are specific to a single connection and should not be forwarded
/// in proxy requests. These are hop-by-hop headers as defined in HTTP specifications.
#[allow(dead_code)]
pub(crate) const CONNECTION_SPECIFIC_HEADERS: &[&str] = &[
    "connection",
    "content-encoding",
    "content-length",
    "transfer-encoding",
];

/// Makes an HTTP request to the Kobo store backend.
///
/// This function handles the low-level mechanics of forwarding a request to the Kobo store,
/// including removing headers that should not be forwarded (Host and Accept-Encoding) and
/// applying a 10-second timeout to prevent hanging requests.
///
/// # Arguments
/// * `client` - The reqwest HTTP client to use for the request
/// * `method` - The HTTP method (GET, POST, etc.)
/// * `url` - The target URL on the Kobo store
/// * `mut headers` - The headers to include in the request (will be modified in-place)
/// * `body` - The request body bytes
/// * `sync_token` - Optional SyncToken to include in the request headers
///
/// # Returns
/// A Result containing either the response from Kobo or a reqwest error
pub(crate) async fn make_request_to_kobo_store(
    client: &Client,
    method: Method,
    url: &str,
    mut headers: HeaderMap,
    body: bytes::Bytes,
    sync_token: Option<&SyncToken>,
) -> Result<ReqwestResponse, reqwest::Error> {
    headers.remove(axum::http::header::HOST);
    headers.remove(axum::http::header::ACCEPT_ENCODING);

    // Add sync token to headers if provided
    if let Some(token) = sync_token {
        let mut token_headers = HashMap::new();
        token.to_headers(&mut token_headers);

        if let Some(token_value) = token_headers.get(SYNC_TOKEN_HEADER) {
            headers.insert(
                axum::http::HeaderName::from_static(SYNC_TOKEN_HEADER),
                axum::http::HeaderValue::from_str(token_value)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
            );
        }
    }

    client
        .request(method, url)
        .headers(headers)
        .body(body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
}

/// Routes requests based on proxy configuration, either returning a success response,
/// performing a temporary redirect, or proxying the request to Kobo.
///
/// # Behavior
/// - If `proxy_kobo_store` is false: returns a 200 OK with empty JSON response
/// - If method is GET: returns a temporary redirect to the target URL
/// - For other methods: proxies the request to Kobo and returns the store's response
///
/// # Arguments
/// * `client` - The reqwest HTTP client for making proxied requests
/// * `proxy_kobo_store` - Whether proxying is enabled
/// * `method` - The HTTP method of the incoming request
/// * `url` - The target URL to redirect/proxy to
/// * `headers` - The incoming request headers
/// * `body` - The incoming request body
/// * `sync_token` - Optional SyncToken to include in proxied requests
///
/// # Returns
/// An HTTP response (as an implementor of IntoResponse)
pub(crate) async fn redirect_or_proxy_request(
    client: &Client,
    proxy_kobo_store: bool,
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Bytes,
    sync_token: Option<&SyncToken>,
) -> impl IntoResponse {
    if !proxy_kobo_store {
        return (StatusCode::OK, Json(serde_json::json!({}))).into_response();
    }

    if method == Method::GET {
        return Redirect::to(url).into_response();
    }

    // Proxy non-GET requests manually
    match make_request_to_kobo_store(client, method, url, headers, body, sync_token).await {
        Ok(store_response) => make_proxy_response(store_response).await.into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

/// Transforms a response from the Kobo store into an HTTP response suitable for forwarding to the client.
///
/// This function extracts the status code and body from the Kobo store response, filters out
/// connection-specific headers (hop-by-hop headers), and constructs a new response to return
/// to the client with the appropriate status and headers.
///
/// # Arguments
/// * `store_response` - The raw response from the Kobo store
///
/// # Returns
/// An HTTP response (as an implementor of IntoResponse)
pub(crate) async fn make_proxy_response(store_response: ReqwestResponse) -> impl IntoResponse {
    let status = store_response.status();
    let mut response_headers = store_response.headers().clone();

    // Remove hop-by-hop headers that should not be forwarded to the client
    for &header in CONNECTION_SPECIFIC_HEADERS {
        response_headers.remove(header);
    }

    match store_response.bytes().await {
        Ok(body_bytes) => {
            let mut response = (status, body_bytes).into_response();
            *response.headers_mut() = response_headers;
            response
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn get_store_url_for_current_request(
    state: AppState,
    uri: &Uri,
    token: &str,
) -> Result<String, AppError> {
    let full_path = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or_default();
    let prefix = format!("/kobo/{token}/");

    let request_path = full_path
        .split_once(&prefix)
        .map(|(_, subpath)| subpath)
        .unwrap_or_else(|| full_path.trim_start_matches('/'));

    let resources = state.kobo_resources.lock().await;

    let parsed_auth = Url::parse(&resources.device_auth)
        .map_err(|e| AppError::Internal(format!("Invalid device_auth URL: {e}")))?;

    let base_origin = parsed_auth.origin().ascii_serialization();

    let raw_url = format!("{base_origin}/{request_path}");
    let validated_url = Url::parse(&raw_url)
        .map_err(|e| AppError::Internal(format!("Invalid target URL '{raw_url}': {e}")))?;

    debug!(
        full_path,
        token,
        request_path,
        base_origin = %base_origin,
        target_url = %validated_url,
        "Resolved target Kobo store URL"
    );
    Ok(validated_url.to_string())
}

/// Get the download URL format for a book.
///
/// # Arguments
/// * `uri` - The incoming request URI
/// * `auth_token` - The Kobo authentication token
///
/// # Returns
/// The download URL format string with placeholders
pub fn get_download_url_format_for_book(uri: &str, auth_token: &str) -> String {
    let uri = Uri::from_str(uri).unwrap_or_else(|_| Uri::from_static("https://localhost"));

    let host = uri
        .authority()
        .map(|auth| {
            let host = auth.host();
            // Strip port if IPv4
            if host.contains(':') && !host.ends_with(']') {
                host.split(':').next().unwrap_or(host)
            } else {
                host
            }
        })
        .unwrap_or("localhost");

    format!(
        "{}://{}/kobo/{}/download/[bookid]/[bookformat]",
        uri.scheme_str().unwrap_or("https"),
        host.trim_end_matches('/'),
        auth_token,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::init_resources::Resources, test_helpers::MongoTestContext};
    use axum::http::Uri;
    use std::str::FromStr;
    use test_context::test_context;
    use test_log::test;

    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    async fn test_get_store_url_for_current_request(
        ctx: &mut MongoTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = ctx.state.clone();
        let mut config = (*state.config).clone();
        config.proxy_kobo_store = true;
        state.config = std::sync::Arc::new(config);

        let mut resources = Resources::default();
        resources.device_auth = "https://storeapi.kobo.com/v1/auth/device".to_string();

        let token = "test-token-123";
        let uri = Uri::from_str("/kobo/test-token-123/v1/user/profile?param=value")?;

        let url = get_store_url_for_current_request(state, &uri, token).await?;

        assert_eq!(url, "https://storeapi.kobo.com/v1/user/profile?param=value");

        Ok(())
    }
}
