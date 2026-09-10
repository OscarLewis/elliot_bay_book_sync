use axum::{body::Body, http::Request, middleware::Next, response::Response};
use tracing::debug;

pub async fn log_with_body(req: Request<Body>, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();

    debug!(
        method = %parts.method,
        uri = %parts.uri,
        headers = ?parts.headers,
        body = ?bytes,
        "Incoming request"
    );

    let req = Request::from_parts(parts, Body::from(bytes.clone()));
    next.run(req).await
}
