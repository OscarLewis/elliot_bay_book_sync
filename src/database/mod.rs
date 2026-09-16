mod book_repository;
pub mod document;
mod scan_repository;
pub use book_repository::BookRepository;
pub use scan_repository::ScanRepository;

use crate::error::AppError;
use mongodb::{Client, Database};

#[derive(Clone)]
pub struct MongoDatabase {
    pub db: Database,
    pub books: BookRepository,
    pub scans: ScanRepository,
}

impl MongoDatabase {
    pub async fn connect(uri: &str, database: &str) -> Result<Self, AppError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(database);

        Ok(Self {
            books: BookRepository::new(&db),
            scans: ScanRepository::new(&db),
            db,
        })
    }

    pub async fn drop_database(&self) -> Result<(), AppError> {
        self.db.drop().await?;
        Ok(())
    }
}
