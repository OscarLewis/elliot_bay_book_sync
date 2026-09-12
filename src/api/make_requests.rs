use axum::{
    Json,
    body::Bytes,
    http::{HeaderMap, Method},
    response::{IntoResponse, Redirect},
};
use reqwest::{Client, Response as ReqwestResponse, StatusCode};

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
///
/// # Returns
/// A Result containing either the response from Kobo or a reqwest error
pub(crate) async fn make_request_to_kobo_store(
    client: &Client,
    method: Method,
    url: &str,
    mut headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<ReqwestResponse, reqwest::Error> {
    // Replicate Python: outgoing_headers.remove("Host")
    headers.remove(axum::http::header::HOST);
    headers.remove(axum::http::header::ACCEPT_ENCODING);

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
) -> impl IntoResponse {
    if !proxy_kobo_store {
        return (StatusCode::OK, Json(serde_json::json!({}))).into_response();
    }

    if method == Method::GET {
        return Redirect::temporary(url).into_response();
        // TODO I think this is incorrect
        /*
        if request.method == "GET":
            return redirect(get_store_url_for_current_request(), 307)


        def get_store_url_for_current_request():
            # Programmatically modify the current url to point to the official Kobo store
            __, __, request_path_with_auth_token = request.full_path.rpartition("/kobo/")
            __, __, request_path = request_path_with_auth_token.rstrip("?").partition(
                "/"
            )
            return KOBO_STOREAPI_URL + "/" + request_path


        '''rust
        if method == Method::GET {
            let kobo_store_url = format!("https://storeapi.kobo.com/{}", request_path);
            return Redirect::to(&kobo_store_url).into_response();
        }
        '''
        */
    }

    // Proxy non-GET requests manually
    match make_request_to_kobo_store(client, method, url, headers, body).await {
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
