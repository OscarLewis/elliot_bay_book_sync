use crate::{error::AppError, library::book::Book};

use mongodb::{Client, Collection, bson::to_document};
pub struct MongoDB {
    pub collection: Collection<mongodb::bson::Document>,
    pub db: mongodb::Database,
}

impl MongoDB {
    pub async fn connect(uri: &str, database: &str) -> Result<Self, AppError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(database);
        let collection = db.collection("books");

        Ok(Self { collection, db })
    }

    pub async fn insert_book(&self, book: &Book) -> Result<(), AppError> {
        let document = to_document(book)?;

        self.collection.insert_one(document).await?;

        Ok(())
    }

    pub async fn drop_database(&self) -> Result<(), AppError> {
        self.db.drop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AppState, config::AppConfig, database::mongodb::MongoDB, error::AppError,
        library::book::Book, scan::scanner::ScanResponse, test_helpers::setup_test_app,
    };
    use axum::http::StatusCode;
    use dotenvy::dotenv;
    use std::{path::PathBuf, sync::Arc};
    use test_log::test;

    #[test(tokio::test)]
    async fn test_mongodb_book_insert() -> Result<(), AppError> {
        dotenv().ok();
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let book = Book::from_path(epub_path.clone());

        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());

        let mongodb = MongoDB::connect(&uri, &database).await?;

        mongodb.insert_book(&book).await?;

        let document = mongodb
            .collection
            .find_one(mongodb::bson::doc! {})
            .await?
            .expect("book should have been inserted");

        assert_eq!(document.get_str("name").unwrap(), book.name);

        // Clean up database at end of test
        mongodb.drop_database().await?;

        Ok(())
    }
}
