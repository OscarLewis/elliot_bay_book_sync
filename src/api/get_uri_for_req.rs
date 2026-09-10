use axum::http::Uri;

pub const KOBO_STOREAPI_URL: &str = "https://storeapi.kobo.com";

// TODO this will get used in HandleCoverImageRequest
pub const _KOBO_IMAGEHOST_URL: &str = "https://cdn.kobo.com/book-images";

/// Parses the incoming request URI to build the official Kobo Store API URL.
pub fn get_store_url_for_current_request(uri: &Uri) -> String {
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");

    // Replicate Python: request.full_path.rpartition("/kobo/")
    if let Some((_, path_after_kobo)) = path_and_query
        .rfind("/kobo/")
        .map(|idx| path_and_query.split_at(idx + 6))
    {
        // Strip trailing '?' if present, then partition on the first '/' to skip the auth_token segment
        let clean_path = path_after_kobo.trim_end_matches('?');

        let request_path = if let Some((_auth_token, relative_path)) = clean_path.split_once('/') {
            relative_path
        } else {
            ""
        };

        format!("{}/{}", KOBO_STOREAPI_URL, request_path)
    } else {
        // Fallback if `/kobo/` wasn't found in the URI path
        format!(
            "{}/{}",
            KOBO_STOREAPI_URL,
            path_and_query.trim_start_matches('/')
        )
    }
}
