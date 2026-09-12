use crate::{error::AppError, library::book::Book, scan::scanner::ScanStatus};
use redb::{Builder, Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

const BOOK_COLLECTION: TableDefinition<&str, &[u8]> = TableDefinition::new("book_docs");
const SCAN_COLLECTION: TableDefinition<&str, &[u8]> = TableDefinition::new("scan_docs");

const BOOK_PATH_INDEX: TableDefinition<&str, &str> = TableDefinition::new("book_path_idx");
const SCAN_TIME_INDEX: TableDefinition<(&str, Uuid), &str> = TableDefinition::new("scan_time_idx");

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
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, AppError> {
        let db = Database::create(path)?;
        Ok(Self { db })
    }

    pub fn open_in_memory() -> Result<Self, AppError> {
        let db = Builder::new().create_with_backend(redb::backends::InMemoryBackend::new())?;
        Ok(Self { db })
    }

    pub fn get_book_by_path<P: AsRef<Path>, T: for<'a> Deserialize<'a>>(
        &self,
        path: P,
    ) -> Result<Option<(String, T)>, AppError> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| AppError::InvalidPath("Path contains invalid UTF-8".into()))?;

        let read_txn = self.db.begin_read()?;

        let index_table = match read_txn.open_table(BOOK_PATH_INDEX) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(AppError::Table(e)),
        };

        let doc_id_guard = match index_table.get(path_str)? {
            Some(guard) => guard,
            None => return Ok(None),
        };
        let doc_id = doc_id_guard.value().to_string();

        let doc_table = read_txn.open_table(BOOK_COLLECTION)?;
        if let Some(doc_guard) = doc_table.get(doc_id.as_str())? {
            let doc: T = serde_json::from_slice(doc_guard.value())?;
            Ok(Some((doc_id, doc)))
        } else {
            Ok(None)
        }
    }

    pub fn get_most_recent_scan<T: for<'a> Deserialize<'a>>(
        &self,
    ) -> Result<Option<(String, T)>, AppError> {
        let read_txn = self.db.begin_read()?;

        let index_table = match read_txn.open_table(SCAN_TIME_INDEX) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(AppError::Table(e)),
        };

        let doc_id = match index_table.iter()?.next_back() {
            Some(result) => {
                let (_key_guard, value_guard) = result?;
                value_guard.value().to_string()
            }
            None => return Ok(None),
        };

        let doc_table = read_txn.open_table(SCAN_COLLECTION)?;
        if let Some(doc_guard) = doc_table.get(doc_id.as_str())? {
            let doc: T = serde_json::from_slice(doc_guard.value())?;
            Ok(Some((doc_id, doc)))
        } else {
            Ok(None)
        }
    }

    pub fn create<T: Serialize>(
        &self,
        target: DocumentTable,
        doc: &T,
        get_path: Option<fn(&T) -> &str>,
        get_timestamp: Option<fn(&T) -> &str>,
    ) -> Result<String, AppError> {
        let id_uuid = Uuid::new_v4();
        let id_str = id_uuid.to_string();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(target.definition())?;
            let payload = serde_json::to_vec(doc)?;
            table.insert(id_str.as_str(), payload.as_slice())?;

            match target {
                DocumentTable::Books => {
                    if let Some(path_fn) = get_path {
                        let mut index_table = write_txn.open_table(BOOK_PATH_INDEX)?;
                        index_table.insert(path_fn(doc), id_str.as_str())?;
                    }
                }
                DocumentTable::Scans => {
                    if let Some(time_fn) = get_timestamp {
                        let mut index_table = write_txn.open_table(SCAN_TIME_INDEX)?;
                        index_table.insert((time_fn(doc), id_uuid), id_str.as_str())?;
                    }
                }
            }
        }
        write_txn.commit()?;
        Ok(id_str)
    }

    pub fn create_many<T: Serialize>(
        &self,
        target: DocumentTable,
        docs: &[T],
        get_path: Option<fn(&T) -> &str>,
        get_timestamp: Option<fn(&T) -> &str>,
    ) -> Result<Vec<String>, AppError> {
        let write_txn = self.db.begin_write()?;
        let mut ids = Vec::with_capacity(docs.len());

        {
            let mut table = write_txn.open_table(target.definition())?;

            let mut path_index_table = if target == DocumentTable::Books && get_path.is_some() {
                Some(write_txn.open_table(BOOK_PATH_INDEX)?)
            } else {
                None
            };

            let mut time_index_table = if target == DocumentTable::Scans && get_timestamp.is_some()
            {
                Some(write_txn.open_table(SCAN_TIME_INDEX)?)
            } else {
                None
            };

            for doc in docs {
                let id_uuid = Uuid::new_v4();
                let id_str = id_uuid.to_string();
                let payload = serde_json::to_vec(doc)?;

                table.insert(id_str.as_str(), payload.as_slice())?;

                if let (Some(idx), Some(path_fn)) = (path_index_table.as_mut(), get_path) {
                    idx.insert(path_fn(doc), id_str.as_str())?;
                }

                if let (Some(idx), Some(time_fn)) = (time_index_table.as_mut(), get_timestamp) {
                    idx.insert((time_fn(doc), id_uuid), id_str.as_str())?;
                }

                ids.push(id_str);
            }
        }

        write_txn.commit()?;
        Ok(ids)
    }

    pub fn update<T: Serialize + for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
        doc: &T,
        get_path: Option<fn(&T) -> &str>,
        get_timestamp: Option<fn(&T) -> &str>,
    ) -> Result<bool, AppError> {
        let id_uuid = Uuid::parse_str(id)?;
        let write_txn = self.db.begin_write()?;
        let mut updated = false;
        {
            let mut table = write_txn.open_table(target.definition())?;

            let old_doc: Option<T> = match table.get(id)? {
                Some(guard) => serde_json::from_slice(guard.value()).ok(),
                None => None,
            };

            if let Some(old_doc) = old_doc {
                match target {
                    DocumentTable::Books => {
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
                    DocumentTable::Scans => {
                        if let Some(time_fn) = get_timestamp {
                            let old_time = time_fn(&old_doc);
                            let new_time = time_fn(doc);

                            let mut index_table = write_txn.open_table(SCAN_TIME_INDEX)?;
                            if old_time != new_time {
                                index_table.remove((old_time, id_uuid))?;
                            }
                            index_table.insert((new_time, id_uuid), id)?;
                        }
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

    pub fn delete<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
        get_path: Option<fn(&T) -> &str>,
        get_timestamp: Option<fn(&T) -> &str>,
    ) -> Result<bool, AppError> {
        let write_txn = self.db.begin_write()?;
        let mut deleted = false;
        {
            let mut table = match write_txn.open_table(target.definition()) {
                Ok(t) => t,
                Err(redb::TableError::TableDoesNotExist(_)) => return Ok(false),
                Err(e) => return Err(AppError::Table(e)),
            };

            let existing_doc: Option<T> = match table.get(id)? {
                Some(guard) => serde_json::from_slice(guard.value()).ok(),
                None => None,
            };

            if let Some(doc) = existing_doc {
                match target {
                    DocumentTable::Books => {
                        if let Some(path_fn) = get_path {
                            if let Ok(mut index_table) = write_txn.open_table(BOOK_PATH_INDEX) {
                                index_table.remove(path_fn(&doc))?;
                            }
                        }
                    }
                    DocumentTable::Scans => {
                        if let Some(time_fn) = get_timestamp {
                            if let Ok(id_uuid) = Uuid::parse_str(id) {
                                if let Ok(mut index_table) = write_txn.open_table(SCAN_TIME_INDEX) {
                                    index_table.remove((time_fn(&doc), id_uuid))?;
                                }
                            }
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

    pub fn read<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
        id: &str,
    ) -> Result<Option<T>, AppError> {
        let read_txn = self.db.begin_read()?;
        let table = match read_txn.open_table(target.definition()) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(err) => return Err(AppError::Table(err)),
        };

        if let Some(guard) = table.get(id)? {
            let doc = serde_json::from_slice(guard.value())?;
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }

    pub fn get_all<T: for<'a> Deserialize<'a>>(
        &self,
        target: DocumentTable,
    ) -> Result<Vec<(String, T)>, AppError> {
        let read_txn = self.db.begin_read()?;

        let table = match read_txn.open_table(target.definition()) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(err) => return Err(AppError::Table(err)),
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

    pub fn get_books_needing_metadata(&self) -> Result<Vec<(String, Book)>, AppError> {
        let read_txn = self.db.begin_read()?;

        let table = match read_txn.open_table(BOOK_COLLECTION) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(err) => return Err(AppError::Table(err)),
        };

        let mut results = Vec::new();

        for result in table.iter()? {
            let (key, value) = result?;

            let id = key.value().to_string();
            let book: Book = serde_json::from_slice(value.value())?;

            if !book.has_metadata {
                results.push((id, book));
            }
        }

        Ok(results)
    }
}
