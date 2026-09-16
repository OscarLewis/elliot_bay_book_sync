use crate::{error::AppError, library::book::Book, scan::scanner::ScanDocument};

use mongodb::{
    Client, Collection,
    bson::{oid::ObjectId, to_document},
};

pub struct MongoDB {
    pub books: Collection<mongodb::bson::Document>,
    pub scans: Collection<mongodb::bson::Document>,
    pub db: mongodb::Database,
}

impl MongoDB {
    pub async fn connect(uri: &str, database: &str) -> Result<Self, AppError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(database);

        let books = db.collection("books");
        let scans = db.collection("scans");

        Ok(Self { books, scans, db })
    }

    pub async fn insert_book(&self, book: &Book) -> Result<ObjectId, AppError> {
        let document = to_document(book)?;
        let result = self.books.insert_one(document).await?;

        result
            .inserted_id
            .as_object_id()
            .ok_or(AppError::InvalidObjectId)
    }

    pub async fn insert_scan(&self, scan: &ScanDocument) -> Result<ObjectId, AppError> {
        let document = to_document(scan)?;
        let result = self.scans.insert_one(document).await?;

        result
            .inserted_id
            .as_object_id()
            .ok_or(AppError::InvalidObjectId)
    }

    pub async fn drop_database(&self) -> Result<(), AppError> {
        self.db.drop().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        database::mongodb::MongoDB,
        error::AppError,
        library::book::Book,
        scan::scanner::{ScanDetails, ScanDocument, ScanStatus},
    };
    use chrono::Utc;
    use dotenvy::dotenv;
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

        let book_id = mongodb.insert_book(&book).await?;

        let document = mongodb
            .books
            .find_one(mongodb::bson::doc! { "_id": book_id })
            .await?
            .expect("book should have been inserted");

        assert_eq!(document.get_str("name").unwrap(), book.name);

        mongodb.drop_database().await?;

        Ok(())
    }

    #[test(tokio::test)]
    async fn test_mongodb_scan_insert() -> Result<(), AppError> {
        dotenv().ok();

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");

        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());

        let mongodb = MongoDB::connect(&uri, &database).await?;

        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now().to_rfc3339(),
            details: ScanDetails::Started,
        };

        let scan_id = mongodb.insert_scan(&scan).await?;

        let document = mongodb
            .scans
            .find_one(mongodb::bson::doc! { "_id": scan_id })
            .await?
            .expect("scan should have been inserted");

        assert_eq!(
            document.get_str("status").unwrap(),
            serde_json::to_string(&scan.status)
                .unwrap()
                .trim_matches('"')
        );

        mongodb.drop_database().await?;

        Ok(())
    }
}
