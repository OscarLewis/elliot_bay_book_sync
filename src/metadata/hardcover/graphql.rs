use graphql_client::GraphQLQuery;
use reqwest::Client;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use tracing::{debug, info};

use crate::error::AppError;

#[allow(non_camel_case_types, dead_code)]
mod scalars {
    // Both casings point to String
    pub type Date = String;
    pub type date = String;

    pub type Timestamp = String;
    pub type timestamptz = String;

    // Both casings point to String
    pub type Numeric = f64;
    pub type numeric = f64;

    pub type Float8 = f64;
    pub type float8 = f64;

    pub type bigint = i64;

    pub type Int = i64;
    pub type int = i64;

    pub type Json = String;
    pub type jsonb = serde_json::Value;
}

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "queries/schema.json",
    query_path = "queries/book_by_pk.graphql",
    custom_scalars_module = "scalars",
    response_derives = "Serialize"
)]
pub struct BookByPK;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "queries/schema.json",
    query_path = "queries/search_books.graphql",
    custom_scalars_module = "scalars",
    response_derives = "Serialize"
)]
pub struct SearchBooks;

pub type HardcoverBookByPK = book_by_pk::BookByPkBooksByPk;
pub async fn book_by_pk_query(
    authorization_token: &str,
    id: i64,
) -> Result<Option<HardcoverBookByPK>, AppError> {
    let client = Client::new();

    let variables = book_by_pk::Variables { id };

    let request_body = BookByPK::build_query(variables);

    let response = client
        .post("https://api.hardcover.app/v1/graphql")
        .bearer_auth(authorization_token)
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response = response
        .json::<graphql_client::Response<book_by_pk::ResponseData>>()
        .await?;

    debug!(?response.errors, "Hardcover response");

    let Some(data) = response.data else {
        return Ok(None);
    };

    Ok(data.books_by_pk)
}

pub type HardcoverBookSearch = search_books::SearchBooksSearch;
pub async fn search_books_query(
    authorization_token: &str,
    query: &str,
) -> Result<Option<HardcoverBookSearch>, AppError> {
    let client = Client::new();

    let variables = search_books::Variables {
        query: query.to_string(),
    };

    let request_body = SearchBooks::build_query(variables);

    let response = client
        .post("https://api.hardcover.app/v1/graphql")
        .bearer_auth(authorization_token)
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response = response
        .json::<graphql_client::Response<search_books::ResponseData>>()
        .await?;

    // Extract the inner `search` field directly
    Ok(response.data.and_then(|d| d.search))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dotenvy::dotenv;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_book_by_pk() -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")?;

        let book: HardcoverBookByPK = book_by_pk_query(&authorization_token, 2168623)
            .await?
            .expect("Book 2168623 should exist");

        assert_eq!(book.id, 2168623);
        assert_eq!(book.slug, Some("absolute-martian-manhunter-vol-1".into()));
        assert_eq!(
            book.title,
            Some("Absolute Martian Manhunter, Vol. 1: Martian Vision".into())
        );

        let file = std::fs::File::create("test_data/test_book_absolute_martian_manhunter.json")?;
        serde_json::to_writer_pretty(file, &book)?;

        Ok(())
    }
    #[test(tokio::test)]
    async fn test_single_book_search() -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")?;
        let search_title = "Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp";

        let books: HardcoverBookSearch = search_books_query(&authorization_token, search_title)
            .await?
            .expect("Search should return books");

        // Assert something about the returned search result.
        assert!(books.results.is_some());

        let file = std::fs::File::create(
            "test_data/test_hardcover_search_absolute_martian_manhunter.json",
        )?;
        serde_json::to_writer_pretty(file, &books)?;

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_multi_book_search() -> Result<(), Box<dyn std::error::Error>> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")?;
        let search_title = "The Wheel of Time";

        let books: HardcoverBookSearch = search_books_query(&authorization_token, search_title)
            .await?
            .expect("Search should return books");

        // Assert something about the returned search result.
        assert!(books.results.is_some());

        let file = std::fs::File::create("test_data/test_hardcover_search_wot.json")?;
        serde_json::to_writer_pretty(file, &books)?;

        Ok(())
    }
}
