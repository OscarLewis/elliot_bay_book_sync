use crate::{error::AppError, library::sync::sync_document::SyncedBookDocument};
use mongodb::{Collection, Database, IndexModel, bson::doc};

#[derive(Clone)]
pub struct SyncRepository {
    collection: Collection<SyncedBookDocument>,
}

// TODO implement Sync Repository
impl SyncRepository {
    pub async fn new(db: &Database) -> Result<Self, AppError> {
        let collection: Collection<SyncedBookDocument> = db.collection("synced_books");

        let index = IndexModel::builder().keys(doc! { "timestamp": -1 }).build();

        collection.create_index(index).await?;

        Ok(Self { collection })
    }
}
