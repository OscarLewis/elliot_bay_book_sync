use crate::{
    AppState,
    api::{images::image_handler_with_quality, middleware::sync_auth_middleware},
    error::AppError,
    library::book::Book,
};
use axum::{
    Router,
    body::Body,
    extract::{OriginalUri, Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, Request, header},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::Utc;
use mongodb::bson::oid::ObjectId;
use serde::Deserialize;
use std::fmt::Write;
use tower::ServiceExt;
use tower_http::services::ServeFile;

const PAGE_SIZE: i64 = 40;
const NAV_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ_TYPE: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

/// Same pattern as `kobo_routes`
/// auth layer is applied last, so every route above it is protected.
pub fn opds_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/opds/{token}", get(root))
        .route("/opds/{token}/", get(root))
        .route("/opds/{token}/all", get(all_books))
        .route("/opds/{token}/download/{book_id}", get(download))
        // Same handler as the Kobo cover route, but without the Governor limiter
        .route(
            "/opds/{token}/cover/{book_uuid}/{width}/{height}/{quality}/{is_greyscale}",
            get(image_handler_with_quality),
        )
        .route_layer(middleware::from_fn_with_state(state, sync_auth_middleware))
}

#[derive(Deserialize)]
struct PageParams {
    page: Option<u64>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn feed_header(
    out: &mut String,
    id: &str,
    title: &str,
    self_href: &str,
    start_href: &str,
    kind: &str,
) {
    let _ = write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog">
<id>{}</id><title>{}</title><updated>{}</updated>
<author><name>ebbooks</name></author>
<link rel="self" href="{}" type="{kind}"/>
<link rel="start" href="{}" type="{NAV_TYPE}"/>
"#,
        esc(id),
        esc(title),
        Utc::now().to_rfc3339(),
        esc(self_href),
        esc(start_href),
    );
}

/// GET /opds/{token}
async fn root(Path(token): Path<String>, OriginalUri(uri): OriginalUri) -> Response {
    let base = format!("/opds/{token}");
    let mut out = String::new();
    feed_header(
        &mut out,
        "urn:ebbooks:root",
        "ebbooks",
        &uri.to_string(),
        &base,
        NAV_TYPE,
    );
    let _ = write!(
        out,
        r#"<entry><title>All books</title><id>urn:ebbooks:all</id><updated>{}</updated>
<content type="text">Browse the whole library</content>
<link rel="subsection" href="{base}/all" type="{ACQ_TYPE}"/></entry>
</feed>"#,
        Utc::now().to_rfc3339(),
    );
    ([(header::CONTENT_TYPE, NAV_TYPE)], out).into_response()
}

/// GET /opds/{token}/all?page=N
async fn all_books(
    State(state): State<AppState>,
    Path(token): Path<String>,
    OriginalUri(uri): OriginalUri,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let skip = (page - 1) * PAGE_SIZE as u64;
    let (books, total) = state.mongodb.books.fetch_page(skip, PAGE_SIZE).await?;

    let base = format!("/opds/{token}");

    let mut out = String::new();
    feed_header(
        &mut out,
        "urn:ebbooks:all",
        "All books",
        &uri.to_string(),
        &base,
        ACQ_TYPE,
    );

    if page > 1 {
        let _ = writeln!(
            out,
            r#"<link rel="previous" href="{base}/all?page={}" type="{ACQ_TYPE}"/>"#,
            page - 1
        );
    }
    if skip + (books.len() as u64) < total as u64 {
        let _ = writeln!(
            out,
            r#"<link rel="next" href="{base}/all?page={}" type="{ACQ_TYPE}"/>"#,
            page + 1
        );
    }

    for book in &books {
        write_entry(&mut out, book, &base);
    }
    out.push_str("</feed>");
    Ok(([(header::CONTENT_TYPE, ACQ_TYPE)], out).into_response())
}

fn mime_for(book: &Book) -> &'static str {
    match book.initial_format.as_deref() {
        Some("epub") | Some("kepub") => "application/epub+zip",
        Some("pdf") => "application/pdf",
        Some("cbz") => "application/vnd.comicbook+zip",
        Some("mobi") => "application/x-mobipocket-ebook",
        _ => "application/octet-stream",
    }
}

fn write_entry(out: &mut String, book: &Book, base: &str) {
    let Some(id) = book.id else { return };
    let hex = id.to_hex();
    let title = book.title.as_deref().unwrap_or(&book.name);

    let _ = write!(
        out,
        "<entry><title>{}</title><id>urn:ebbooks:book:{hex}</id><updated>{}</updated>",
        esc(title),
        book.modified_at.to_rfc3339()
    );
    if let Some(author) = &book.author {
        let _ = write!(out, "<author><name>{}</name></author>", esc(author));
    }

    // OPDS has no standard series field, so surface it in the summary
    let mut summary = String::new();
    if let Some(series) = &book.series_name {
        match book.series_position {
            Some(pos) => {
                let _ = write!(summary, "{series} #{pos}\n\n");
            }
            None => {
                let _ = write!(summary, "{series}\n\n");
            }
        }
    }
    if let Some(desc) = &book.description {
        summary.push_str(desc);
    }
    if !summary.is_empty() {
        let _ = write!(out, r#"<summary type="text">{}</summary>"#, esc(&summary));
    }

    // The image handler returns 404 if has_image is false, so only link when present
    if book.has_image {
        let _ = write!(
            out,
            r#"<link rel="http://opds-spec.org/image" href="{base}/cover/{hex}/600/900/85/false" type="image/jpeg"/>"#
        );
        let _ = write!(
            out,
            r#"<link rel="http://opds-spec.org/image/thumbnail" href="{base}/cover/{hex}/150/225/75/false" type="image/jpeg"/>"#
        );
    }
    let _ = write!(
        out,
        r#"<link rel="http://opds-spec.org/acquisition" href="{base}/download/{hex}" type="{}"/>"#,
        mime_for(book)
    );
    out.push_str("</entry>\n");
}

/// GET /opds/{token}/download/{book_id}
async fn download(
    State(state): State<AppState>,
    Path((_token, book_id)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let oid = ObjectId::parse_str(&book_id).map_err(|_| AppError::InvalidObjectId)?;
    let book = state
        .mongodb
        .books
        .find_by_id(oid)
        .await?
        .ok_or_else(|| AppError::NotFound("Book not found in database".into()))?;

    let filename = book
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::NotFound("Invalid book filename".into()))?
        .to_string();

    // ServeFile streams from disk and handles Range / conditional requests
    let mut request = Request::builder()
        .method(method)
        .uri("/")
        .body(Body::empty())?;
    *request.headers_mut() = headers;

    let mut response = ServeFile::new(&book.path)
        .oneshot(request)
        .await
        .into_response();

    if let Ok(v) = HeaderValue::from_str(&format!(
        r#"attachment; filename="{}""#,
        filename.replace('"', "")
    )) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use crate::{
        library::book::Book,
        test_helpers::{AppTestContext, setup_test_app, write_minimal_epub},
    };
    use axum::http::StatusCode;
    use std::sync::Arc;
    use test_context::test_context;
    use test_log::test;

    const TOKEN: &str = "test-token-123";

    fn set_token(ctx: &mut AppTestContext) {
        let mut config = (*ctx.state.config).clone();
        config.ebbooks_auth_key = TOKEN.to_string();
        ctx.state.config = Arc::new(config);
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_root_and_bad_token(ctx: &mut AppTestContext) {
        set_token(ctx);
        let server = setup_test_app(ctx.state.clone());

        server
            .get(&format!("/opds/{TOKEN}"))
            .await
            .assert_status(StatusCode::OK);

        server
            .get("/opds/wrong")
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_all_feed_lists_book(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        set_token(ctx);
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.epub");
        write_minimal_epub(&path, "Test")?;

        let mut book = Book::from_path(path);
        book.title = Some("Dungeons & Dragons".into());
        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let server = setup_test_app(ctx.state.clone());
        let resp = server.get(&format!("/opds/{TOKEN}/all")).await;
        resp.assert_status(StatusCode::OK);

        let body = resp.text();
        assert!(body.contains("Dungeons &amp; Dragons"));
        assert!(body.contains(&format!("/opds/{TOKEN}/download/{book_id}")));
        Ok(())
    }

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_download(ctx: &mut AppTestContext) -> Result<(), Box<dyn std::error::Error>> {
        set_token(ctx);
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.epub");
        write_minimal_epub(&path, "Test")?;
        let expected = tokio::fs::read(&path).await?;

        let mut book = Book::from_path(path);
        let book_id = ctx.state.mongodb.books.insert(&mut book).await?;

        let server = setup_test_app(ctx.state.clone());
        let resp = server
            .get(&format!("/opds/{TOKEN}/download/{book_id}"))
            .await;
        resp.assert_status(StatusCode::OK);
        assert_eq!(resp.into_bytes().as_ref(), expected.as_slice());
        Ok(())
    }
}
