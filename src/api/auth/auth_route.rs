use crate::{AppState, config, error::AppError};
use axum::{
    extract,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;
use tracing::debug;

pub async fn auth_request_handler(
    extract::Path(token): extract::Path<String>,
    extract::State(state): extract::State<AppState>,
) -> Result<Response, AppError> {
    debug!(
        token,
        proxy_kobo_store = state.config.proxy_kobo_store,
        "Received auth request"
    );
    if state.config.proxy_kobo_store {}
    Ok(StatusCode::OK.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::init_resources::Resources,
        config::AppConfig,
        database::document::{DocumentDB, DocumentTable},
        library::book::Book,
        metadata::extract_images::extract_imgs_for_books,
        test_helpers::setup_test_app,
    };
    use reqwest::StatusCode;
    use std::{path::PathBuf, sync::Arc};
    use test_log::test;
    use tokio::sync::Mutex;

    #[test(tokio::test)]
    async fn test_auth_handler() -> Result<(), Box<dyn std::error::Error>> {
        let config = AppConfig::default();
        let db = DocumentDB::open_in_memory()?;

        let state = AppState {
            config: Arc::new(config),
            req_client: reqwest::Client::new(),
            db: Arc::new(db),
            kobo_resources: Arc::new(Mutex::new(Resources::default())),
            hardcover_api_token: None,
        };

        let server = setup_test_app(state);

        let token = "test-token-123";

        let response = server.post(&format!("/kobo/{token}/v1/auth/device")).await;

        Ok(())
    }
}

/*
@csrf.exempt
@kobo.route("/v1/auth/device", methods=["POST"])
@requires_kobo_auth
def HandleAuthRequest():
    log.debug('Kobo Auth request')
    if config.config_kobo_proxy:
        try:
            return redirect_or_proxy_request()
        except Exception:
            log.error("Failed to receive or parse response from Kobo's auth endpoint. Falling back to un-proxied mode.")
    return make_calibre_web_auth_response()
    def make_calibre_web_auth_response():

# As described in kobo_auth.py, CalibreWeb doesn't make use practical use of this auth/device API call for
# authentation (nor for authorization). We return a dummy response just to keep the device happy.
content = request.get_json()
AccessToken = base64.b64encode(os.urandom(24)).decode('utf-8')
RefreshToken = base64.b64encode(os.urandom(24)).decode('utf-8')
return make_response(
    jsonify(
        {
            "AccessToken": AccessToken,
            "RefreshToken": RefreshToken,
            "TokenType": "Bearer",
            "TrackingId": str(uuid.uuid4()),
            "UserKey": content.get('UserKey', ""),
        }
    )
)
*/
