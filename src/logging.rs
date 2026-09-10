use axum::{
    body::{Body, Bytes},
    http::Request,
    middleware::Next,
    response::Response,
};
use tracing::{debug, info, trace};

pub async fn log_with_body(req: Request<Body>, next: Next) -> Response {
    let (parts, body) = req.into_parts();

    // Provide a max size for buffering
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();

    trace!("Incoming request: {} {}", parts.method, parts.uri);

    for (name, value) in parts.headers.iter() {
        trace!("Header: {} = {:?}", name, value);
    }

    trace!("Body: {:?}", bytes);

    // Rebuild the request so handlers can still consume the body
    let req = Request::from_parts(parts, Body::from(bytes.clone()));

    next.run(req).await
}
