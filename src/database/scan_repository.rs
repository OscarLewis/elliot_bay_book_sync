use crate::{error::AppError, scan::scanner::ScanDocument};
use mongodb::{
    Collection, Database,
    bson::{doc, oid::ObjectId},
};

#[derive(Clone)]
pub struct ScanRepository {
    collection: Collection<ScanDocument>,
}

impl ScanRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("scans"),
        }
    }

    pub async fn insert(&self, scan: &ScanDocument) -> Result<ObjectId, AppError> {
        let result = self.collection.insert_one(scan).await?;
        result
            .inserted_id
            .as_object_id()
            .ok_or(AppError::InvalidObjectId)
    }

    pub async fn find_by_id(&self, id: ObjectId) -> Result<Option<ScanDocument>, AppError> {
        Ok(self.collection.find_one(doc! { "_id": id }).await?)
    }

    pub async fn latest(&self) -> Result<Option<ScanDocument>, AppError> {
        Ok(self
            .collection
            .find_one(doc! {})
            .sort(doc! { "timestamp": -1 })
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        database::MongoDatabase,
        error::AppError,
        scan::scanner::{ScanDetails, ScanDocument, ScanStatus},
    };
    use chrono::Utc;
    use dotenvy::dotenv;
    use test_log::test;

    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_insert() -> Result<(), AppError> {
        dotenv().ok();

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now().to_rfc3339(),
            details: ScanDetails::Started,
        };

        let scan_id = mongodb.scans.insert(&scan).await?;

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should have been inserted");

        assert_eq!(found.status, scan.status);

        mongodb.drop_database().await?;
        Ok(())
    }
}
