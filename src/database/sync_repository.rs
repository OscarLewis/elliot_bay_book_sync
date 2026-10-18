use crate::{error::AppError, library::sync::sync_document::SyncedBookDocument};
use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::ReplaceOptions,
};

#[derive(Clone)]
pub struct SyncRepository {
    collection: Collection<SyncedBookDocument>,
}

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

    pub async fn upsert(&self, synced_book: &mut SyncedBookDocument) -> Result<(), AppError> {
        let filter = doc! {
            "book_id": &synced_book.book_id,
            "user_id": &synced_book.user_id
        };

        let result = self
            .collection
            .replace_one(filter, &*synced_book)
            .upsert(true)
            .await?;

        if let Some(id) = result.upserted_id.and_then(|id| id.as_object_id()) {
            synced_book.id = Some(id);
        }

        Ok(())
    }
}
