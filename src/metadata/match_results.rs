use strsim::jaro_winkler;
use tracing::debug;

use crate::{
    error::AppError,
    metadata::{epub::parse::EpubDiskMetadata, fetch_meta::IntermediateBookSearchResult},
};

pub async fn match_metadata_for_book(
    book_title: &str,
    search_results: Vec<IntermediateBookSearchResult>,
    epub_metadata: &EpubDiskMetadata,
) -> Result<IntermediateBookSearchResult, AppError> {
    debug!(
        book_title,
        ?epub_metadata,
        num_results = search_results.len(),
        "Beggining analysis on book title"
    );

    search_results
        .into_iter()
        .map(|result| {
            let title = title_score(epub_metadata.title.as_deref(), &result);
            let author = author_score(epub_metadata.author.as_deref(), &result);
            let series = series_score(epub_metadata.series.as_deref(), &result);
            let position = series_position_score(epub_metadata.series_position, &result);

            let total = title + author + series + position;

            debug!(
                result_id = result.id,
                result_title = %result.title,
                title,
                author,
                series,
                position,
                total,
                "Candidate match score"
            );

            (result, total)
        })
        .max_by(|(_, a_score), (_, b_score)| a_score.total_cmp(b_score))
        .map(|(result, _)| result)
        .ok_or_else(|| AppError::Internal("No search results found".to_string()))
}

fn title_score(epub_title: Option<&str>, result: &IntermediateBookSearchResult) -> f64 {
    fn normalize_title(title: &str) -> String {
        title
            .chars()
            .flat_map(char::to_lowercase)
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .filter(|word| !matches!(*word, "epub" | "mobi" | "azw" | "azw3" | "pdf"))
            .filter(|word| !word.chars().all(|c| c.is_ascii_digit()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    let Some(epub_title) = epub_title else {
        return 0.5;
    };

    let epub_title = normalize_title(epub_title);
    let result_title = normalize_title(&result.title);

    jaro_winkler(&epub_title, &result_title)
}

fn author_score(epub_author: Option<&str>, result: &IntermediateBookSearchResult) -> f64 {
    fn normalize_author(author: &str) -> String {
        author
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let Some(epub_author) = epub_author else {
        return 0.5;
    };

    let epub_author = normalize_author(epub_author);

    result
        .author_names
        .iter()
        .map(|author| {
            let author = normalize_author(author);
            let similarity = jaro_winkler(&epub_author, &author);

            if similarity >= 0.95 {
                similarity
            } else if similarity >= 0.80 {
                similarity * 0.5
            } else {
                similarity * 0.1
            }
        })
        .max_by(f64::total_cmp)
        .unwrap_or(0.5)
}

fn series_score(epub_series: Option<&str>, result: &IntermediateBookSearchResult) -> f64 {
    fn normalize_series(series: &str) -> String {
        series
            .chars()
            .flat_map(char::to_lowercase)
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let Some(epub_series) = epub_series else {
        return 0.5;
    };

    let epub_series = normalize_series(epub_series);

    let mut best_score: f64 = 0.0;

    for series in &result.series_names {
        let series = normalize_series(series);

        let similarity = jaro_winkler(&epub_series, &series);

        best_score = best_score.max(similarity);
    }

    if let Some(featured_series) = &result.featured_series {
        if let Some(series) = &featured_series.series {
            if let Some(name) = &series.name {
                let name = normalize_series(name);

                let similarity = jaro_winkler(&epub_series, &name);

                best_score = best_score.max(similarity);
            }
        }
    }

    best_score
}

fn series_position_score(epub_position: Option<f64>, result: &IntermediateBookSearchResult) -> f64 {
    match (
        epub_position,
        result.featured_series.as_ref().and_then(|s| s.position),
    ) {
        (Some(epub), Some(result)) if (epub - result).abs() < f64::EPSILON => 1.0,
        (Some(_), Some(_)) => 0.0,
        _ => 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{epub::parse::parse_metadata_ebook, fetch_meta::fetch_metadata_for_book};
    use dotenvy::dotenv;
    use std::path::PathBuf;
    use test_log::test;
    use tracing::debug;

    #[test(tokio::test)]
    async fn test_match_metadata_for_hunger_games() -> Result<(), AppError> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")
            .map_err(|e| AppError::Internal(format!("HARDCOVER_TOKEN env var not set: {e}")))?;

        let book_title = "Suzanne Collins - Hunger Games 01-The Hunger Games (epub)";
        let path = PathBuf::from(
            "test ebooks/Suzanne Collins - Hunger Games 01-The Hunger Games (epub).epub",
        );

        let metadata = parse_metadata_ebook(path).await?;

        let search_results =
            fetch_metadata_for_book(&authorization_token, book_title, &metadata).await?;

        let result = match_metadata_for_book(book_title, search_results, &metadata).await?;

        debug!(?result, "Matched result");

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_match_metadata_for_martian_manhunter() -> Result<(), AppError> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")
            .map_err(|e| AppError::Internal(format!("HARDCOVER_TOKEN env var not set: {e}")))?;

        let book_title = "Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp";
        let path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        let metadata = parse_metadata_ebook(path).await?;

        let search_results =
            fetch_metadata_for_book(&authorization_token, book_title, &metadata).await?;

        let result = match_metadata_for_book(book_title, search_results, &metadata).await?;

        debug!(?result, "Matched result");

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_match_metadata_for_cats_cradle() -> Result<(), AppError> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")
            .map_err(|e| AppError::Internal(format!("HARDCOVER_TOKEN env var not set: {e}")))?;

        let book_title = "Cat's Cradle - Kurt Vonnegut";
        let path = PathBuf::from("test ebooks/Cat's Cradle - Kurt Vonnegut.kepub.epub");

        let metadata = parse_metadata_ebook(path).await?;

        let search_results =
            fetch_metadata_for_book(&authorization_token, book_title, &metadata).await?;

        let result = match_metadata_for_book(book_title, search_results, &metadata).await?;

        debug!(?result, "Matched result");

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_match_metadata_for_lions_of_al_rassan() -> Result<(), AppError> {
        dotenv().ok();

        let authorization_token = std::env::var("HARDCOVER_TOKEN")
            .map_err(|e| AppError::Internal(format!("HARDCOVER_TOKEN env var not set: {e}")))?;

        let book_title = "The Lions of Al-Rassan - Guy Gavriel Kay";
        let path = PathBuf::from("test ebooks/The Lions of Al-Rassan - Guy Gavriel Kay.epub");

        let metadata = parse_metadata_ebook(path).await?;

        let search_results =
            fetch_metadata_for_book(&authorization_token, book_title, &metadata).await?;

        let result = match_metadata_for_book(book_title, search_results, &metadata).await?;

        debug!(?result, "Matched result");

        Ok(())
    }
}
