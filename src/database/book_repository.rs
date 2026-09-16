use crate::{error::AppError, library::book::Book};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::IndexOptions,
};

#[derive(Clone)]
pub struct BookRepository {
    collection: Collection<Book>,
}

impl BookRepository {
    pub async fn new(db: &Database) -> Result<Self, AppError> {
        let collection: Collection<Book> = db.collection("books");

        let index = IndexModel::builder()
            .keys(doc! { "path": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build();

        collection.create_index(index).await?;

        Ok(Self { collection })
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

    /// Verifies that inserting a `Book` persists it to MongoDB and that it can
    /// be retrieved by the `ObjectId` returned from the insert.
    ///
    /// Setup: writes a dummy 2KB `.epub` file to a temp directory, builds a
    /// `Book` from its path, and inserts it into a fresh, uniquely-named test
    /// database.
    ///
    /// Asserts: the document found by `find_by_id` has the same `name` as the
    /// original `Book`.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_insert() -> Result<(), AppError> {
        // Load MONGODB_TEST_URI from .env if present (no-op if already set).
        dotenv().ok();

        // Create a scratch directory for a fake epub file; dropped automatically
        // when `temp_dir` goes out of scope at the end of the test.
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");

        // Write 2KB of zero bytes — contents don't matter, only that the file exists.
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        // Build the Book from the fake file path.
        let book = Book::from_path(epub_path.clone());

        // Use a randomly-named database so parallel/repeated test runs don't collide.
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        // Insert the book and capture the generated ObjectId.
        let book_id = mongodb.books.insert(&book).await?;

        // Read it back by id to confirm it was actually written.
        let found = mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("book should have been inserted");

        // The round-tripped document should match what we inserted.
        assert_eq!(found.name, book.name);

        // Clean up the test database so it doesn't linger on the server.
        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that the unique index on `Book::path` rejects a second insert
    /// of a book with the same path.
    ///
    /// Setup: writes a single dummy `.epub` file and builds one `Book` from
    /// it, then inserts that same `Book` twice into a fresh test database.
    ///
    /// Asserts: the first insert succeeds; the second insert returns an
    /// error, since `BookRepository::new` creates a unique index on `path`
    /// at construction time.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_path_unique_index() -> Result<(), AppError> {
        dotenv().ok();

        // Same fake epub setup as above — only one Book is built from it,
        // since this test only cares about inserting the same path twice.
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());

        // Connecting also runs BookRepository::new, which creates the unique
        // index on `path` before any inserts happen below.
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let book = Book::from_path(epub_path.clone());

        // First insert should succeed — nothing else has this path yet.
        mongodb.books.insert(&book).await?;

        // Second insert of the exact same Book (same path) should be rejected
        // by MongoDB's unique index, surfacing as an AppError.
        let second_insert = mongodb.books.insert(&book).await;

        assert!(second_insert.is_err(), "duplicate path should be rejected");

        mongodb.drop_database().await?;
        Ok(())
    }
}
