use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use tracing::debug;

#[derive(Deserialize)]
pub struct TokenParam {
    token: String,
}

/// Middleware to check if the provided token matches the configured auth key.
/// If the token is valid, the request is passed to the next handler;
/// otherwise, a 401 Unauthorized response is returned.
pub async fn sync_auth_middleware(
    Path(TokenParam { token }): Path<TokenParam>,
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if token == state.config.ebbooks_auth_key {
        debug!("Sync auth middleware passed for token: {}", token);
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
