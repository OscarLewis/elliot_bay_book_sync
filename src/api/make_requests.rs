use axum::{
    Json,
    body::Bytes,
    http::{HeaderMap, Method},
    response::{IntoResponse, Redirect},
};
use reqwest::{Client, Response as ReqwestResponse, StatusCode};

#[allow(dead_code)]
pub(crate) const CONNECTION_SPECIFIC_HEADERS: &[&str] = &[
    "connection",
    "content-encoding",
    "content-length",
    "transfer-encoding",
];

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
    }

    // Proxy non-GET requests manually
    match make_request_to_kobo_store(client, method, url, headers, body).await {
        Ok(store_response) => make_proxy_response(store_response).await.into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

pub(crate) async fn make_proxy_response(store_response: ReqwestResponse) -> impl IntoResponse {
    let status = store_response.status();
    let mut response_headers = store_response.headers().clone();

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
