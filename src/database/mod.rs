mod book_repository;
pub mod document;
mod scan_repository;
pub use book_repository::BookRepository;
pub use scan_repository::ScanRepository;
pub mod bson_chrono_datetime;
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
            books: BookRepository::new(&db).await?,
            scans: ScanRepository::new(&db).await?,
            db,
        })
    }

    // pub async fn drop_database(&self) -> Result<(), AppError> {
    //     self.db.drop().await?;
    //     Ok(())
    // }

    /// Drops the entire underlying database. Test-only — there's no
    /// legitimate reason to call this in production code.
    pub async fn drop_database(&self) -> mongodb::error::Result<()> {
        self.db.drop().await
    }
}
