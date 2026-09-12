use crate::{
    error::AppError,
    metadata::{epub::parse::EpubDiskMetadata, search_books_query},
};
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Deserialize, Clone)]
pub struct FeaturedSeriesInfo {
    pub position: Option<f64>,
    pub series: Option<SeriesDetails>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SeriesDetails {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub books_count: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BookImage {
    pub url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub id: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IntermediateBookSearchResult {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: u64,
    pub title: String,
    pub slug: String,
    pub description: Option<String>,

    #[serde(default)]
    pub image: Option<BookImage>,

    // Primary series names array from the search document
    #[serde(default)]
    pub series_names: Vec<String>,

    #[serde(default)]
    pub author_names: Vec<String>,

    // Detailed series info if this book is part of a featured/primary series
    pub featured_series: Option<FeaturedSeriesInfo>,
}

/// Custom deserializer to handle both string IDs ("2077352") and integer IDs (2077352)
#[allow(dead_code)]
fn deserialize_id<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("Invalid integer ID")),
        serde_json::Value::String(s) => s
            .parse::<u64>()
            .map_err(|_| serde::de::Error::custom("Failed to parse string ID as i64")),
        _ => Err(serde::de::Error::custom(
            "Expected string or integer for ID",
        )),
    }
}

#[derive(Deserialize)]
struct SearchHit {
    document: IntermediateBookSearchResult,
}

#[derive(Deserialize)]
struct SearchHitsContainer {
    hits: Vec<SearchHit>,
}

pub async fn fetch_metadata_for_book(
    authorization_token: &str,
    book_title: &str,
    epub_metadata: &EpubDiskMetadata,
) -> Result<Vec<IntermediateBookSearchResult>, AppError> {
    let result = search_books_query(authorization_token, book_title).await?;

    let search_results = result
        .and_then(|s| s.results)
        .and_then(|v| {
            // debug!(?v, "Raw Hardcover search results");

            match serde_json::from_value::<SearchHitsContainer>(v) {
                Ok(container) => Some(container),
                Err(err) => {
                    debug!(?err, "Failed to deserialize Hardcover search results");
                    None
                }
            }
        })
        .map(|container| {
            for hit in &container.hits {
                debug!(?hit.document, "Search result document");
            }

            container
                .hits
                .into_iter()
                .map(|h| h.document)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(search_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::epub::parse::parse_metadata_ebook;
    use dotenvy::dotenv;
    use std::path::PathBuf;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_fetch_metadata_for_book() -> Result<(), AppError> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")
            .map_err(|e| AppError::Internal(format!("HARDCOVER_TOKEN env var not set: {e}")))?;

        let path = PathBuf::from(
            "test ebooks/Suzanne Collins - Hunger Games 01-The Hunger Games (epub).epub",
        );

        let metadata = parse_metadata_ebook(path).await?;

        let search_title = "Hunger Games 01-The Hunger Games";

        fetch_metadata_for_book(&authorization_token, search_title, &metadata).await?;

        Ok(())
    }
}
