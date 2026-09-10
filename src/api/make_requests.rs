use crate::api::init_resources::ResourcesRoot;
use axum::http::{HeaderMap, Method};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct KoboStoreResponse {
    pub(crate) response_status: Option<ResponseStatus>,
    pub(crate) resources: Option<ResourcesRoot>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ResponseStatus {
    pub(crate) error_code: Option<String>,
    pub(crate) message: Option<String>,
}
