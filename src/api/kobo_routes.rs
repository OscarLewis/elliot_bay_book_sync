use crate::{AppState, api::initialization::initialization_handler};
use axum::{Router, routing::get};

/// Return a `Router<AppState>` so it can be merged with the main app router.
pub fn kobo_routes() -> Router<AppState> {
    Router::new().route(
        "/kobo/{token}/v1/initialization",
        get(initialization_handler),
    )
}
