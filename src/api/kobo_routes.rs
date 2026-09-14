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
    library::sync::{
        download_handler::{self, download_request_handler},
        get_tests::get_tests_handler,
        metadata_handler::metadata_request_handler,
        reading_state_handler::reading_state_handler,
        stubs::{dummy_proxy_handler, wishlist_stub_handler},
        sync_handler::library_sync_handler,
    },
};
use axum::{
    Router,
    routing::{any, get, post, put},
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
        .route(
            "/kobo/{token}/cover/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}",
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
        .route(
            "/kobo/{token}/v1/library/{book_uuid}/state",
            get(reading_state_handler).put(reading_state_handler),
        )
        .route(
            "/kobo/{token}/v1/library/{book_uuid}/metadata",
            get(metadata_request_handler),
        )
        .route(
            "/kobo/{token}/download/{book_id}/{book_format}",
            get(download_request_handler),
        )
        .route(
            "/kobo/{token}/v1/analytics/gettests",
            get(get_tests_handler).post(get_tests_handler),
        ) // Stubbed / Proxied Kobo Store Routes
        .route(
            "/kobo/{token}/v1/user/loyalty/{*subpath}",
            any(dummy_proxy_handler),
        )
        .route("/kobo/{token}/v1/user/profile", any(dummy_proxy_handler))
        .route("/kobo/{token}/v1/user/wishlist", get(wishlist_stub_handler))
        .route(
            "/kobo/{token}/v1/user/recommendations",
            any(dummy_proxy_handler),
        )
        .route(
            "/kobo/{token}/v1/analytics/{*subpath}",
            any(dummy_proxy_handler),
        )
        .route("/kobo/{token}/v1/assets", any(dummy_proxy_handler))
}
