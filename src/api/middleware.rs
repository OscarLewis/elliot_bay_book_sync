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
