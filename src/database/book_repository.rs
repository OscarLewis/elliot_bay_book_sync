use crate::{error::AppError, library::book::Book};
use futures_util::stream::TryStreamExt;
use mongodb::{
    Collection, Database, IndexModel,
    bson::{self, Document, doc, oid::ObjectId},
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

    pub async fn find_by_path(&self, path: &str) -> Result<Option<(ObjectId, Book)>, AppError> {
        let Some(book) = self.collection.find_one(doc! { "path": path }).await? else {
            return Ok(None);
        };

        let id = book.id.ok_or(AppError::InvalidObjectId)?;

        Ok(Some((id, book)))
    }

    pub async fn fetch_all(&self) -> Result<Vec<Book>, AppError> {
        let mut cursor = self.collection.find(doc! {}).await?;
        let mut books = Vec::new();

        while let Some(book) = cursor.try_next().await? {
            books.push(book);
        }

        Ok(books)
    }

    pub async fn delete(&self, id: ObjectId) -> Result<bool, AppError> {
        let result = self.collection.delete_one(doc! { "_id": id }).await?;
        Ok(result.deleted_count == 1)
    }

    /// Inserts multiple `Book` documents into the database in a single batch request.
    ///
    /// Assigns the generated `ObjectId` to each `Book` and preserves the input sequence.
    /// Returns an empty vector without making a database call if `books` is empty.
    pub async fn insert_many(&self, books: &mut [Book]) -> Result<Vec<ObjectId>, AppError> {
        if books.is_empty() {
            return Ok(Vec::new());
        }

        let result = self.collection.insert_many(&*books).await?;

        // Extract ObjectIds mapping from index order to preserve input sequence.
        let mut ids = Vec::with_capacity(result.inserted_ids.len());

        for i in 0..books.len() {
            let id = result
                .inserted_ids
                .get(&i)
                .and_then(|bson| bson.as_object_id())
                .ok_or(AppError::InvalidObjectId)?;

            books[i].id = Some(id);
            ids.push(id);
        }

        Ok(ids)
    }

    /// Updates only the fields of `book` that differ from the existing document in MongoDB.
    /// Returns `Ok(true)` if the document existed and was updated, `Ok(false)` otherwise.
    pub async fn update_diff(&self, id: ObjectId, book: &Book) -> Result<bool, AppError> {
        // Fetch the raw document from MongoDB
        let raw_collection = self.collection.clone_with_type::<Document>();
        let existing = match raw_collection.find_one(doc! { "_id": id }).await? {
            Some(doc) => doc,
            None => return Ok(false),
        };

        //  Convert the incoming Book into a BSON Document
        let new_doc = bson::to_document(book)?;

        // Diff fields between current DB state and new struct
        let mut set_fields = Document::new();

        for (key, new_val) in new_doc {
            if key == "_id" {
                continue;
            }

            match existing.get(&key) {
                Some(old_val) if old_val == &new_val => {
                    // Unchanged, skip
                }
                _ => {
                    // New or modified field
                    set_fields.insert(key, new_val);
                }
            }
        }

        // If nothing changed, skip DB roundtrip
        if set_fields.is_empty() {
            return Ok(true);
        }

        // Apply only the diff via $set
        let result = self
            .collection
            .update_one(doc! { "_id": id }, doc! { "$set": set_fields })
            .await?;

        Ok(result.matched_count == 1)
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

    pub async fn find_books_needing_metadata(&self) -> Result<Vec<Book>, AppError> {
        let mut cursor = self.collection.find(doc! { "has_metadata": false }).await?;

        let mut books = Vec::new();

        while let Some(book) = cursor.try_next().await? {
            books.push(book);
        }

        Ok(books)
    }

    pub async fn find_books_needing_images(&self) -> Result<Vec<Book>, AppError> {
        let mut cursor = self.collection.find(doc! { "has_image": false }).await?;

        let mut books = Vec::new();

        while let Some(book) = cursor.try_next().await? {
            books.push(book);
        }

        Ok(books)
    }

    pub async fn delete_all(&self) -> Result<(), AppError> {
        self.collection.delete_many(doc! {}).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::AppError, library::book::Book, test_helpers::test_mongodb};
    use mongodb::bson::doc;
    use test_log::test;

    /// Verifies that inserting a `Book` persists it to MongoDB and that it can
    /// be retrieved by the `ObjectId` returned from the insert.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_insert() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let book = Book::from_path(epub_path);

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let book_id = mongodb.books.insert(&book).await?;
            inserted_ids.push(book_id);

            let found = mongodb
                .books
                .find_by_id(book_id)
                .await?
                .expect("book should have been inserted");

            assert_eq!(found.name, book.name);
            Ok(())
        };

        let result = run_test.await;

        // Clean up only the inserted test records
        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }

    /// Verifies that the unique index on `Book::path` rejects a second insert
    /// of a book with the same path.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_path_unique_index() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let book = Book::from_path(epub_path);

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let book_id = mongodb.books.insert(&book).await?;
            inserted_ids.push(book_id);

            let second_insert = mongodb.books.insert(&book).await;
            assert!(second_insert.is_err(), "duplicate path should be rejected");

            Ok(())
        };

        let result = run_test.await;

        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }

    /// Verifies that `update` overwrites an existing document's fields and
    /// reports `true` when a match was found.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let original = Book::from_path(epub_path.clone());

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let book_id = mongodb.books.insert(&original).await?;
            inserted_ids.push(book_id);

            let mut updated = Book::from_path(epub_path.clone());
            updated.name = "Renamed Book".to_string();

            let was_updated = mongodb.books.update(book_id, &updated).await?;
            assert!(was_updated, "update should report a matched document");

            let found = mongodb
                .books
                .find_by_id(book_id)
                .await?
                .expect("book should still exist after update");

            assert_eq!(found.name, updated.name);
            Ok(())
        };

        let result = run_test.await;

        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }

    /// Verifies that `update` reports `false` when no document matches the
    /// given id, rather than erroring or silently inserting.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_missing_returns_false() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let book = Book::from_path(epub_path);

        let missing_id = mongodb::bson::oid::ObjectId::new();
        let was_updated = mongodb.books.update(missing_id, &book).await?;

        assert!(!was_updated, "update should report no matched document");

        Ok(())
    }

    /// Verifies that `update_title` with `Some(title)` sets the `title`
    /// field without disturbing other fields on the document.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_title_some() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let book = Book::from_path(epub_path);
        let original_name = book.name.clone();

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let book_id = mongodb.books.insert(&book).await?;
            inserted_ids.push(book_id);

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
            assert_eq!(found.name, original_name);
            Ok(())
        };

        let result = run_test.await;

        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }

    /// Verifies that `update_title` with `None` removes the `title` field
    /// from the document entirely.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_update_title_none() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let epub_path = temp_dir.path().join("test_name.epub");
        tokio::fs::write(&epub_path, vec![0u8; 2 * 1024]).await?;

        let mongodb = test_mongodb().await;
        let mut book = Book::from_path(epub_path);
        book.title = Some("Original Title".to_string());

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let book_id = mongodb.books.insert(&book).await?;
            inserted_ids.push(book_id);

            let was_updated = mongodb.books.update_title(book_id, None).await?;
            assert!(was_updated, "update_title should report a matched document");

            let found = mongodb
                .books
                .find_by_id(book_id)
                .await?
                .expect("book should still exist after update");

            assert_eq!(found.title, None);
            Ok(())
        };

        let result = run_test.await;

        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }

    /// Verifies that `insert_many` persists multiple `Book` documents at once and
    /// returns their generated `ObjectId`s in matching order.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_book_insert_many() -> Result<(), AppError> {
        let temp_dir = tempfile::tempdir()?;
        let path_a = temp_dir.path().join("book_a.epub");
        let path_b = temp_dir.path().join("book_b.epub");
        tokio::fs::write(&path_a, vec![0u8; 1024]).await?;
        tokio::fs::write(&path_b, vec![0u8; 1024]).await?;

        let mongodb = test_mongodb().await;

        let book_a = Book::from_path(path_a);
        let book_b = Book::from_path(path_b);
        let mut books = vec![book_a.clone(), book_b.clone()];

        let mut inserted_ids = Vec::new();

        let run_test = async {
            let ids = mongodb.books.insert_many(&mut books).await?;
            assert_eq!(ids.len(), 2);
            inserted_ids.extend(&ids);

            let found_a = mongodb
                .books
                .find_by_id(ids[0])
                .await?
                .expect("first book should exist");
            let found_b = mongodb
                .books
                .find_by_id(ids[1])
                .await?
                .expect("second book should exist");

            assert_eq!(found_a.name, book_a.name);
            assert_eq!(found_b.name, book_b.name);

            let empty_ids = mongodb.books.insert_many(&mut []).await?;
            assert!(empty_ids.is_empty());

            Ok(())
        };

        let result = run_test.await;

        for id in inserted_ids {
            let _ = mongodb.books.delete(id).await;
        }

        result
    }
}
