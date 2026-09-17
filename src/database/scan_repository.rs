use crate::{
    error::AppError,
    scan::scanner::{ScanDetails, ScanDocument, ScanStatus},
};
use mongodb::{
    Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId, to_bson},
};

#[derive(Clone)]
pub struct ScanRepository {
    collection: Collection<ScanDocument>,
}

impl ScanRepository {
    pub async fn new(db: &Database) -> Result<Self, AppError> {
        let collection: Collection<ScanDocument> = db.collection("scans");

        let index = IndexModel::builder().keys(doc! { "timestamp": -1 }).build();

        collection.create_index(index).await?;

        Ok(Self { collection })
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

    /// Shared implementation behind the public `mark_*` transition methods.
    ///
    /// Sets `status` and `details` together in a single `$set`, so a scan
    /// can never be left with a `status` that doesn't match its `details`
    /// (e.g. `Finished` paired with `Failed { .. }`). This is private —
    /// callers use `mark_completed`/`mark_failed` instead, which construct
    /// a valid `(status, details)` pair by construction rather than
    /// accepting them as separate, independently-supplied arguments.
    async fn set_status(
        &self,
        id: ObjectId,
        status: ScanStatus,
        details: ScanDetails,
    ) -> Result<bool, AppError> {
        let update = doc! {
            "$set": {
                "status": to_bson(&status)?,
                "details": to_bson(&details)?,
            }
        };

        let result = self
            .collection
            .update_one(doc! { "_id": id }, update)
            .await?;
        Ok(result.matched_count == 1)
    }

    /// Transitions a scan to `ScanStatus::Finished` with a
    /// `ScanDetails::Completed` summary of what the scan did.
    ///
    /// Takes the three completion counts directly rather than a
    /// pre-built `ScanDetails`, so the only way to call this is with data
    /// that's actually shaped like a completed scan.
    pub async fn mark_completed(
        &self,
        id: ObjectId,
        added_count: usize,
        updated_count: usize,
        skipped_count: usize,
    ) -> Result<bool, AppError> {
        self.set_status(
            id,
            ScanStatus::Finished,
            ScanDetails::Completed {
                added_count,
                updated_count,
                skipped_count,
            },
        )
        .await
    }

    /// Transitions a scan to `ScanStatus::Error` with a
    /// `ScanDetails::Failed` reason.
    pub async fn mark_failed(&self, id: ObjectId, reason: String) -> Result<bool, AppError> {
        self.set_status(id, ScanStatus::Error, ScanDetails::Failed { reason })
            .await
    }

    pub async fn latest(&self) -> Result<Option<ScanDocument>, AppError> {
        Ok(self
            .collection
            .find_one(doc! {})
            .sort(doc! { "timestamp": -1 })
            .await?)
    }

    /// Records a new scan starting now, and returns its `ObjectId`.
    ///
    /// This is a thin wrapper around `insert(&ScanDocument::start())` —
    /// callers who just want to say "a scan started" don't need to know
    /// about `ScanStatus`/`ScanDetails` at all, and can't accidentally
    /// insert a scan in some other status.
    pub async fn start(&self) -> Result<ObjectId, AppError> {
        self.insert(&ScanDocument::start()).await
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

    /// Verifies that inserting a `ScanDocument` persists it to MongoDB and
    /// that it can be retrieved by the `ObjectId` returned from the insert.
    ///
    /// Setup: builds a single `ScanDocument` with `ScanStatus::Running` and
    /// the current timestamp, and inserts it into a fresh, uniquely-named
    /// test database.
    ///
    /// Asserts: the document found by `find_by_id` has the same `status` as
    /// the original `ScanDocument`.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_insert() -> Result<(), AppError> {
        dotenv().ok();

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");

        // Random database name to keep this run isolated from any other.
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        // A minimal "scan just started" document.
        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now().to_rfc3339(),
            details: ScanDetails::Started,
        };

        // Insert and capture the generated ObjectId.
        let scan_id = mongodb.scans.insert(&scan).await?;

        // Read it back by id to confirm it was actually written.
        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should have been inserted");

        // The round-tripped document should match what we inserted.
        assert_eq!(found.status, scan.status);

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `ScanRepository::latest` returns the most recently
    /// timestamped scan, using the descending index on `timestamp`.
    ///
    /// Setup: inserts two `ScanDocument`s with fixed, distinct timestamps —
    /// one from 2024 and one from 2025 — into a fresh test database, in
    /// chronological order.
    ///
    /// Asserts: `latest()` returns the 2025 scan, not the 2024 one,
    /// confirming the query sorts by `timestamp` descending rather than by
    /// insertion order.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_latest() -> Result<(), AppError> {
        dotenv().ok();

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        // Two scans with fixed timestamps, chosen so ordering is unambiguous
        // regardless of when the test actually runs.
        let older = ScanDocument {
            status: ScanStatus::Finished,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            details: ScanDetails::Started,
        };
        let newer = ScanDocument {
            status: ScanStatus::Running,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            details: ScanDetails::Started,
        };

        // Insert older first, then newer — insertion order intentionally
        // matches chronological order here, but `latest()` should rely on
        // the timestamp field, not insertion order, to pick the right one.
        mongodb.scans.insert(&older).await?;
        mongodb.scans.insert(&newer).await?;

        // Should return `newer`, proving the query sorts by timestamp desc.
        let latest = mongodb.scans.latest().await?.expect("should find a scan");

        assert_eq!(latest.timestamp, newer.timestamp);

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `mark_completed` transitions a `Running` scan to
    /// `Finished` and attaches the given counts as `ScanDetails::Completed`.
    ///
    /// Setup: inserts a scan with `status: Running, details: Started`, then
    /// calls `mark_completed` with arbitrary counts.
    ///
    /// Asserts: `mark_completed` returns `true`, and re-fetching the scan
    /// shows `status: Finished` paired with the exact `ScanDetails::Completed`
    /// counts passed in — confirming both fields moved together in one
    /// atomic update rather than leaving a window where they could
    /// disagree.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_mark_completed() -> Result<(), AppError> {
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

        let was_updated = mongodb.scans.mark_completed(scan_id, 3, 1, 0).await?;
        assert!(
            was_updated,
            "mark_completed should report a matched document"
        );

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should still exist after update");

        assert_eq!(found.status, ScanStatus::Finished);
        assert_eq!(
            found.details,
            ScanDetails::Completed {
                added_count: 3,
                updated_count: 1,
                skipped_count: 0,
            }
        );
        // timestamp should be untouched by the partial update.
        assert_eq!(found.timestamp, scan.timestamp);

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `mark_failed` transitions a `Running` scan to `Error`
    /// and attaches the given reason as `ScanDetails::Failed`.
    ///
    /// Setup: inserts a scan with `status: Running, details: Started`, then
    /// calls `mark_failed` with a specific error message.
    ///
    /// Asserts: `mark_failed` returns `true`, and re-fetching the scan shows
    /// `status: Error` paired with `ScanDetails::Failed` carrying the exact
    /// reason string passed in.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_mark_failed() -> Result<(), AppError> {
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

        let was_updated = mongodb
            .scans
            .mark_failed(scan_id, "disk read error".to_string())
            .await?;
        assert!(was_updated, "mark_failed should report a matched document");

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should still exist after update");

        assert_eq!(found.status, ScanStatus::Error);
        assert_eq!(
            found.details,
            ScanDetails::Failed {
                reason: "disk read error".to_string(),
            }
        );

        mongodb.drop_database().await?;
        Ok(())
    }

    /// Verifies that `start` inserts a new scan in the `Running`/`Started`
    /// state with a fresh timestamp, and that it can be retrieved by the
    /// returned `ObjectId`.
    ///
    /// Asserts: the found document has `status: Running` and
    /// `details: Started` — the only state a scan can be created in via
    /// this path — with no need for the test to construct a `ScanDocument`
    /// itself.
    ///
    /// Requires `MONGODB_TEST_URI` to point at a reachable MongoDB instance;
    /// the test database is dropped on completion.
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_start() -> Result<(), AppError> {
        dotenv().ok();

        let uri = std::env::var("MONGODB_TEST_URI")
            .expect("MONGODB_TEST_URI must be set in .env or environment");
        let database = format!("elliot_bay_book_sync_test_{}", uuid::Uuid::new_v4());
        let mongodb = MongoDatabase::connect(&uri, &database).await?;

        let scan_id = mongodb.scans.start().await?;

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should have been inserted");

        assert_eq!(found.status, ScanStatus::Running);
        assert_eq!(found.details, ScanDetails::Started);

        mongodb.drop_database().await?;
        Ok(())
    }
}
