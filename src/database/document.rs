use redb::{Builder, Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

// Primary document tables
const BOOK_COLLECTION: TableDefinition<&str, &[u8]> = TableDefinition::new("book_docs");
const SCAN_COLLECTION: TableDefinition<&str, &[u8]> = TableDefinition::new("scan_docs");

// Secondary index table: maps Book Path -> Book UUID
const BOOK_PATH_INDEX: TableDefinition<&str, &str> = TableDefinition::new("book_path_idx");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentTable {
    Books,
    Scans,
}

impl DocumentTable {
    fn definition(&self) -> TableDefinition<&'static str, &'static [u8]> {
        match self {
            DocumentTable::Books => BOOK_COLLECTION,
            DocumentTable::Scans => SCAN_COLLECTION,
        }
    }
}

pub struct DocumentDB {
    db: Database,
}

impl DocumentDB {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, redb::Error> {
        let db = Database::create(path)?;
        Ok(Self { db })
    }

    pub fn open_in_memory() -> Result<Self, redb::Error> {
        let db = Builder::new().create_with_backend(redb::backends::InMemoryBackend::new())?;
        Ok(Self { db })
    }

    /// Book Lookup by Path
    /// Fetches a book document directly using its path index in O(1)
    pub fn get_book_by_path<P: AsRef<Path>, T: for<'a> Deserialize<'a>>(
        &self,
        path: P,
    ) -> Result<Option<(String, T)>, Box<dyn std::error::Error>> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or("Path contains invalid UTF-8")?;

        let read_txn = self.db.begin_read()?;

        // 1. Look up the index table
        let index_table = match read_txn.open_table(BOOK_PATH_INDEX) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(Box::new(e)),
        };

        let doc_id_guard = match index_table.get(path_str)? {
            Some(guard) => guard,
            None => return Ok(None),
        };
        let doc_id = doc_id_guard.value().to_string();

        // 2. Fetch the actual document from the primary table
        let doc_table = read_txn.open_table(BOOK_COLLECTION)?;
        if let Some(doc_guard) = doc_table.get(doc_id.as_str())? {
            let doc: T = serde_json::from_slice(doc_guard.value())?;
            Ok(Some((doc_id, doc)))
        } else {
            Ok(None)
        }
    }

    /// CREATE: Inserts a document and maintains path index if path getter is provided.
    pub fn create<T: Serialize>(
        &self,
        target: DocumentTable,
        doc: &T,
        get_path: Option<fn(&T) -> &str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let id = Uuid::new_v4().to_string();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(target.definition())?;
            let payload = serde_json::to_vec(doc)?;
            table.insert(id.as_str(), payload.as_slice())?;

            if target == DocumentTable::Books {
                if let Some(path_fn) = get_path {
                    let mut index_table = write_txn.open_table(BOOK_PATH_INDEX)?;
                    index_table.insert(path_fn(doc), id.as_str())?;
                }
            }
        }
        write_txn.commit()?;
        Ok(id)
    }

    /// BATCH CREATE: Inserts multiple documents and updates secondary indexes in a single transaction.
    pub fn create_many<T: Serialize>(
        &self,
        target: DocumentTable,
        docs: &[T],
        get_path: Option<fn(&T) -> &str>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        let mut ids = Vec::with_capacity(docs.len());

        {
            let mut table = write_txn.open_table(target.definition())?;

            // Open index table separately if targeting Books with path extractor
            let mut index_table = if target == DocumentTable::Books && get_path.is_some() {
                Some(write_txn.open_table(BOOK_PATH_INDEX)?)
            } else {
                None
            };

            for doc in docs {
                let id = Uuid::new_v4().to_string();
                let payload = serde_json::to_vec(doc)?;

                // 1. Insert into primary document table
                table.insert(id.as_str(), payload.as_slice())?;

                // 2. Insert into index table if present
                if let (Some(idx), Some(path_fn)) = (index_table.as_mut(), get_path) {
                    idx.insert(path_fn(doc), id.as_str())?;
                }

                ids.push(id);
            }
        }

        write_txn.commit()?;
        Ok(ids)
    }

    /// UPDATE: Overwrites an existing document and updates path index if the path changed.
    pub fn update<T: Serialize + for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
        doc: &T,
        get_path: Option<fn(&T) -> &str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        let mut updated = false;
        {
            let mut table = write_txn.open_table(target.definition())?;

            // 1. Fetch old document and release the immutable table borrow immediately
            let old_doc: Option<T> = match table.get(id)? {
                Some(guard) => serde_json::from_slice(guard.value()).ok(),
                None => None,
            };

            // 2. Perform mutable operations now that guard is dropped
            if let Some(old_doc) = old_doc {
                if target == DocumentTable::Books {
                    if let Some(path_fn) = get_path {
                        let old_path = path_fn(&old_doc);
                        let new_path = path_fn(doc);

                        let mut index_table = write_txn.open_table(BOOK_PATH_INDEX)?;
                        if old_path != new_path {
                            index_table.remove(old_path)?;
                        }
                        index_table.insert(new_path, id)?;
                    }
                }

                let payload = serde_json::to_vec(doc)?;
                table.insert(id, payload.as_slice())?;
                updated = true;
            }
        }
        write_txn.commit()?;
        Ok(updated)
    }

    /// DELETE: Removes a document and cleans up its path index entry.
    pub fn delete<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
        get_path: Option<fn(&T) -> &str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let write_txn = self.db.begin_write()?;
        let mut deleted = false;
        {
            let mut table = match write_txn.open_table(target.definition()) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(false),
                Err(e) => return Err(Box::new(e)),
            };

            // Retrieve old document and drop table guard
            let existing_doc: Option<T> = match table.get(id)? {
                Some(guard) => serde_json::from_slice(guard.value()).ok(),
                None => None,
            };

            // Perform index cleanup and removal
            if let Some(doc) = existing_doc {
                if target == DocumentTable::Books {
                    if let Some(path_fn) = get_path {
                        if let Ok(mut index_table) = write_txn.open_table(BOOK_PATH_INDEX) {
                            index_table.remove(path_fn(&doc))?;
                        }
                    }
                }
                table.remove(id)?;
                deleted = true;
            }
        }
        write_txn.commit()?;
        Ok(deleted)
    }

    /// READ: Fetches a single document by ID.
    pub fn read<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
    ) -> Result<Option<T>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(target.definition()) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(err) => return Err(Box::new(err)),
        };

        if let Some(guard) = table.get(id)? {
            let doc = serde_json::from_slice(guard.value())?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    /// READ ALL: Iterates over all entries in the table.
    pub fn get_all<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
    ) -> Result<Vec<(String, T)>, Box<dyn std::error::Error>> {
        let read_txn = self.db.begin_read()?;

        let table = match read_txn.open_table(target.definition()) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(err) => return Err(Box::new(err)),
        };

        let mut results = Vec::new();
        for result in table.iter()? {
            let (key, value) = result?;
            let id = key.value().to_string();
            let doc: T = serde_json::from_slice(value.value())?;
            results.push((id, doc));
        }

        Ok(results)
    }
}
