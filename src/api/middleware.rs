use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::debug;

pub async fn sync_auth_middleware(
    Path(token): Path<String>,
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
