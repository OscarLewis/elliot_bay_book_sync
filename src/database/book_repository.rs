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

    /// Replaces the entire document for `id` with `book`.
    ///
    /// This is a full replace, not a partial `$set` — every field on the
    /// existing document is overwritten with the fields on `book` (the `_id`
    /// itself is left alone by MongoDB regardless of what's in the
    /// replacement). Since `Book` is typed and doesn't carry its own `_id`
    /// field, passing `&Book` directly to `replace_one` works the same way
    /// `insert` does with `insert_one`.
    ///
    /// Returns `true` if a document with this `id` existed and was replaced,
    /// `false` if no document matched (e.g. the id doesn't exist). This is a
    /// deliberate choice over erroring on no-match, mirroring how `delete`
    /// reports `deleted_count` rather than treating "not found" as failure.
    ///
    // TODO Add an update for metadata fields specifically
    // If I later need to update only a subset of fields, move towards using
    // `update_one` with a `$set` document instead of `replace_one` — that
    // avoids overwriting fields the caller didn't intend to touch.
    pub async fn update(&self, id: ObjectId, book: &Book) -> Result<bool, AppError> {
        let result = self
            .collection
            .replace_one(doc! { "_id": id }, book)
            .await?;

        Ok(result.matched_count == 1)
    }

    /// Sets or clears `Book::title` on the document matching `id`.
    ///
    /// Unlike `update`, which replaces the whole document via `replace_one`,
    /// this uses `update_one` with a `$set` operator document containing
    /// only the `title` field.
    ///
    /// `title` is `Option<String>` on `Book`, so this handles both cases:
    /// - `Some(title)` → `$set: { title: title }`, storing the new value.
    /// - `None` → `$unset: { title: "" }`, removing the field entirely
    ///   rather than setting it to BSON `null`. This matters if `Book`
    ///   is deserialized with `#[serde(default)]` or similar, where a
    ///   missing field and a `null` field aren't guaranteed to behave the
    ///   same way on read.
    ///
    pub async fn update_title(
        &self,
        id: ObjectId,
        title: Option<String>,
    ) -> Result<bool, AppError> {
        let update = match title {
            Some(title) => doc! { "$set": { "title": title } },
            None => doc! { "$unset": { "title": "" } },
        };

        let result = self
            .collection
            .update_one(doc! { "_id": id }, update)
            .await?;

        Ok(result.matched_count == 1)
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

    /// Verifies that `update` overwrites an existing document's fields and
    /// reports `true` when a match was found.
    ///
    /// Setup: inserts a `Book`, then builds a second `Book` from the same
    /// path but with a different `name`, and calls `update` with the
    /// original document's id.
    ///
    /// Asserts: `update` returns `true`, and re-fetching by id shows the new
    /// `name` rather than the original one — confirming the replace actually
    /// took effect server-side rather than just returning success locally.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update() -> Result<(), AppError> {
        dotenv().ok();

        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        // Insert the original book and remember its id.
        let original = Book::from_path(epub_path.clone());
        let book_id = mongodb.books.insert(&original).await?;

        // Build a replacement with the same path (so it still matches the
        // unique index) but a different name, simulating an edited record.
        let mut updated = Book::from_path(epub_path.clone());
        updated.name = "Renamed Book".to_string();

        let was_updated = mongodb.books.update(book_id, &updated).await?;
        assert!(was_updated, "update should report a matched document");

        // Re-fetch to confirm the change was actually persisted, not just
        // reported as successful.
        let found = mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("book should still exist after update");

        assert_eq!(found.name, updated.name);

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `update` reports `false` when no document matches the
    /// given id, rather than erroring or silently inserting.
    ///
    /// Setup: builds a `Book` and calls `update` with a freshly-generated
    /// `ObjectId` that was never inserted.
    ///
    /// Asserts: `update` returns `false`, since `replace_one` with no
    /// matching filter is a no-op rather than an upsert (the driver only
    /// upserts when explicitly configured to).
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_missing_returns_false() -> Result<(), AppError> {
        dotenv().ok();

        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let book = Book::from_path(epub_path.clone());

        // An id that was never inserted, so nothing should match.
        let missing_id = mongodb::bson::oid::ObjectId::new();

        let was_updated = mongodb.books.update(missing_id, &book).await?;
        assert!(!was_updated, "update should report no matched document");

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `update_title` with `Some(title)` sets the `title`
    /// field without disturbing other fields on the document.
    ///
    /// Setup: inserts a `Book` with `title: None`, then calls `update_title`
    /// with `Some("New Title".to_string())`.
    ///
    /// Asserts: `update_title` returns `true`, and re-fetching the book by
    /// id shows the new title while `name` is unchanged from the original
    /// insert — confirming this was a targeted `$set`, not a full replace.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_title_some() -> Result<(), AppError> {
        dotenv().ok();

        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let book = Book::from_path(epub_path.clone());
        let original_name = book.name.clone();
        let book_id = mongodb.books.insert(&book).await?;

        let was_updated = mongodb
            .books
            .update_title(book_id, Some("New Title".to_string()))
            .await?;
        assert!(was_updated, "update_title should report a matched document");

        let found = mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("book should still exist after update");

        assert_eq!(found.title.as_deref(), Some("New Title"));
        // Untouched fields should survive the partial update.
        assert_eq!(found.name, original_name);

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `update_title` with `None` removes the `title` field
    /// from the document entirely, rather than setting it to `null`.
    ///
    /// Setup: inserts a `Book` with a title already set, then calls
    /// `update_title` with `None`.
    ///
    /// Asserts: `update_title` returns `true`, and re-fetching the book
    /// deserializes `title` back to `None` — this holds whether `Book`
    /// uses `$unset` semantics or stores an explicit `null`, but the
    /// `$unset` branch specifically confirms the field is absent from the
    /// raw document, not just falsy.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_title_none() -> Result<(), AppError> {
        dotenv().ok();

        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let mut book = Book::from_path(epub_path.clone());
        book.title = Some("Original Title".to_string());
        let book_id = mongodb.books.insert(&book).await?;

        let was_updated = mongodb.books.update_title(book_id, None).await?;
        assert!(was_updated, "update_title should report a matched document");

        let found = mongodb
            .books
            .find_by_id(book_id)
            .await?
            .expect("book should still exist after update");

        assert_eq!(found.title, None);

        mongodb.drop_database().await?;
        Ok(())
    }
}
