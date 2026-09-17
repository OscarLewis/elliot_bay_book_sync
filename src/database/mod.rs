mod book_repository;
mod scan_repository;
mod sync_repository;
pub use book_repository::BookRepository;
pub use scan_repository::ScanRepository;
pub mod bson_chrono_datetime;
use crate::error::AppError;
use mongodb::{Client, Database};
pub use sync_repository::SyncRepository;

#[derive(Clone)]
pub struct MongoDatabase {
    pub db: Database,
    pub books: BookRepository,
    pub scans: ScanRepository,
    pub syncs: SyncRepository,
}

impl MongoDatabase {
    pub async fn connect(uri: &str, database: &str) -> Result<Self, AppError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(database);

        Ok(Self {
            books: BookRepository::new(&db).await?,
            scans: ScanRepository::new(&db).await?,
            syncs: SyncRepository::new(&db).await?,
            db,
        })
    }

    /// Drops the entire underlying database. Test-only — there's no
    /// legitimate reason to call this in production code.
    pub async fn drop_database(&self) -> mongodb::error::Result<()> {
        self.db.drop().await
    }
}
