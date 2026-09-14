use crate::{
    AppState,
    api::{
        auth::{
            device_auth_route::device_auth_request_handler, oauth::oauth_token_handler,
            oidc::oidc_well_known_configuration_handler,
        },
        images::{image_handler, image_handler_with_quality},
        initialization::initialization_handler,
    },
    library::sync::sync_handler::library_sync_handler,
};
use axum::{
    Router,
    routing::{get, post},
};

/// Return a `Router<AppState>` so it can be merged with the main app router
pub fn kobo_routes() -> Router<AppState> {
    Router::new()
        // Initialization
        .route(
            "/kobo/{token}/v1/initialization",
            get(initialization_handler),
        )
        // Image Routes
        .route(
            "/kobo/{token}/{book_uuid}/{width}/{height}/{isGreyscale}/image.jpg",
            get(image_handler),
        )
        .route(
            "/kobo/{token}/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}/image.jpg",
            get(image_handler_with_quality),
        )
        // Auth Routes
        .route(
            "/kobo/{token}/v1/auth/device",
            post(device_auth_request_handler),
        )
        // Matches /kobo/{token}/oauth/token, /kobo/{token}/oauth/refresh, /kobo/{token}/oauth/foo/bar, etc.
        .route(
            "/kobo/{token}/oauth/{*subpath}",
            get(oauth_token_handler).post(oauth_token_handler),
        )
        .route(
            "/kobo/{token}/oauth/.well-known/openid-configuration",
            get(oidc_well_known_configuration_handler),
        )
        .route("/kobo/{token}/v1/library/sync", get(library_sync_handler))
}
