use axum::http::{HeaderMap, Method};
use reqwest::{Client, Response};

pub(crate) async fn make_request_to_kobo_store(
    client: &Client,
    method: Method,
    url: &str,
    mut headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Response, reqwest::Error> {
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
