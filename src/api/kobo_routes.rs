use crate::{
    AppState,
    api::{
        auth::auth_route::auth_request_handler,
        images::{image_handler, image_handler_with_quality},
        initialization::initialization_handler,
    },
};
use axum::{
    Router,
    routing::{get, post},
};

/// Return a `Router<AppState>` so it can be merged with the main app router
pub fn kobo_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/kobo/{token}/v1/initialization",
            get(initialization_handler),
        )
        .route(
            "/kobo/{token}/{book_uuid}/{width}/{height}/{isGreyscale}/image.jpg",
            get(image_handler),
        )
        .route(
            "/kobo/{token}/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}/image.jpg",
            get(image_handler_with_quality),
        )
        .route("/kobo/{token}/v1/auth/device", post(auth_request_handler))
}
