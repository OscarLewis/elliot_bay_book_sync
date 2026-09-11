use std::path::PathBuf;

use epub::doc::EpubDoc;

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

    let Some((cover_data, mime)) = doc.get_cover() else {
        return Ok(None);
    };

    let extension = match mime.as_str() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => {
            return Err(AppError::Internal(format!(
                "Unsupported cover image MIME type: {mime}"
            )));
        }
    };

    let output_path = output_path.with_extension(extension);

    tokio::fs::write(&output_path, cover_data).await?;

    Ok(Some(output_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test(tokio::test)]
    async fn test_parse_metadata_ebook() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(
            "test ebooks/Suzanne Collins - Hunger Games 01-The Hunger Games (epub).epub",
        );

        let metadata = parse_metadata_ebook(path).await?;

        println!("{metadata:#?}");

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

        let cover_path = cover_path.unwrap();

        assert!(cover_path.exists());

        println!("Cover extracted to: {}", cover_path.display());

        Ok(())
    }
}
