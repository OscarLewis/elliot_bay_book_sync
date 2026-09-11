use std::path::PathBuf;

use epub::doc::EpubDoc;
use image::ImageFormat;

use crate::error::AppError;

#[derive(Debug)]
pub struct EpubDiskMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub series: Option<String>,
    pub series_position: Option<f64>,
}

pub async fn parse_metadata_ebook(path: PathBuf) -> Result<EpubDiskMetadata, AppError> {
    let mut doc = EpubDoc::new(path)?;

    let series = doc
        .mdata("belongs-to-collection")
        .or_else(|| doc.mdata("calibre:series"))
        .map(|item| item.value.clone());

    let series_position = doc
        .mdata("group-position")
        .or_else(|| doc.mdata("calibre:series_index"))
        .and_then(|item| item.value.parse::<f64>().ok());

    Ok(EpubDiskMetadata {
        title: doc.mdata("title").map(|item| item.value.clone()),
        author: doc.mdata("creator").map(|item| item.value.clone()),
        series,
        series_position,
    })
}

pub async fn extract_epub_cover(
    path: PathBuf,
    output_path: PathBuf,
) -> Result<Option<PathBuf>, AppError> {
    let mut doc = EpubDoc::new(path)?;

    let Some((cover_data, _mime)) = doc.get_cover() else {
        return Ok(None);
    };

    let output_path = output_path.with_extension("webp");

    let result = tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&cover_data)
            .map_err(|err| AppError::Internal(format!("Failed to decode cover: {err}")))?;

        image
            .save_with_format(&output_path, ImageFormat::WebP)
            .map_err(|err| AppError::Internal(format!("Failed to save WebP cover: {err}")))?;

        Ok::<PathBuf, AppError>(output_path)
    })
    .await
    .map_err(|err| AppError::Internal(format!("Cover extraction task failed: {err}")))??;

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;
    use tracing::debug;

    #[test(tokio::test)]
    async fn test_parse_metadata_ebook() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(
            "test ebooks/Suzanne Collins - Hunger Games 01-The Hunger Games (epub).epub",
        );

        let metadata = parse_metadata_ebook(path).await?;

        debug!("{metadata:#?}");

        assert!(metadata.title.is_some());
        assert!(metadata.author.is_some());

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_extract_cover() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(
            "test ebooks/Absolute Martian Manhunter Vol. 1_ Martian Vision - Deniz Camp.epub",
        );

        let output_path = PathBuf::from("test_data/test-cover");

        let cover_path = extract_epub_cover(path, output_path).await?;

        assert!(cover_path.is_some());

        let cover_path_disk = cover_path.unwrap();

        // WebP is the best image format on the world wide web. Everyone agrees with me on this.
        assert_eq!(cover_path_disk.extension().unwrap(), "webp");

        assert!(cover_path_disk.exists());

        debug!("Cover extracted to: {}", cover_path_disk.display());

        Ok(())
    }
}
