use crate::{error::AppError, library::book::Book};
use mongodb::{
    Collection, Database,
    bson::{doc, oid::ObjectId},
};

#[derive(Clone)]
pub struct BookRepository {
    collection: Collection<Book>,
}

impl BookRepository {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("books"),
        }
    }

    pub async fn insert(&self, book: &Book) -> Result<ObjectId, AppError> {
        let result = self.collection.insert_one(book).await?;
        result
            .inserted_id
            .as_object_id()
            .ok_or(AppError::InvalidObjectId)
    }

    pub async fn find_by_id(&self, id: ObjectId) -> Result<Option<Book>, AppError> {
        Ok(self.collection.find_one(doc! { "_id": id }).await?)
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Book>, AppError> {
        Ok(self.collection.find_one(doc! { "name": name }).await?)
    }

    pub async fn delete(&self, id: ObjectId) -> Result<bool, AppError> {
        let result = self.collection.delete_one(doc! { "_id": id }).await?;
        Ok(result.deleted_count == 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::{database::MongoDatabase, error::AppError, library::book::Book};
    use dotenvy::dotenv;
    use test_log::test;

    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_insert() -> Result<(), AppError> {
        dotenv().ok();

        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");

        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let book = Book::from_path(epub_path.clone());
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let book_id = mongodb.books.insert(&book).await?;

        let found = mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("book should have been inserted");

        assert_eq!(found.name, book.name);

        mongodb.drop_database().await?;
        Ok(())
    }
}
