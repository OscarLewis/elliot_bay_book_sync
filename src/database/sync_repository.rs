use crate::{error::AppError, library::sync::sync_document::SyncedBookDocument};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
};

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

    pub async fn fetch_all(&self) -> Result<Vec<SyncedBookDocument>, AppError> {
        let mut cursor = self.collection.find(doc! {}).await?;
        let mut synced_books = Vec::new();

        while let Some(synced_book) = cursor.try_next().await? {
            synced_books.push(synced_book);
        }

        Ok(synced_books)
    }

    pub async fn insert(&self, synced_book: &mut SyncedBookDocument) -> Result<ObjectId, AppError> {
        let result = self.collection.insert_one(&*synced_book).await?;

        let id = result
            .inserted_id
            .as_object_id()
            .ok_or(AppError::InvalidObjectId)?;

        synced_book.id = Some(id);

        Ok(id)
    }

    pub async fn delete_by_book_id(&self, book_id: &str) -> Result<(), AppError> {
        self.collection
            .delete_many(doc! { "book_id": book_id })
            .await?;

        Ok(())
    }
}
