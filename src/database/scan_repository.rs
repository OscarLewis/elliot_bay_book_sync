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

    pub async fn delete(&self, id: ObjectId) -> Result<bool, AppError> {
        let result = self.collection.delete_one(doc! { "_id": id }).await?;
        Ok(result.deleted_count == 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::AppError,
        scan::scanner::{ScanDetails, ScanDocument, ScanStatus},
        test_helpers::MongoTestContext,
    };
    use chrono::{DateTime, DurationRound, Utc};
    use test_context::test_context;
    use test_log::test;

    /// Verifies that inserting a `ScanDocument` persists it to MongoDB and
    /// that it can be retrieved by the `ObjectId` returned from the insert.
    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_insert(ctx: &mut MongoTestContext) -> Result<(), AppError> {
        let mongodb = &ctx.state.mongodb;

        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now(),
            details: ScanDetails::Started,
        };

        let scan_id = mongodb.scans.insert(&scan).await?;

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should have been inserted");

        assert_eq!(found.status, scan.status);
        Ok(())
    }

    /// Verifies that `ScanRepository::latest` returns the most recently
    /// timestamped scan, using the descending index on `timestamp`.
    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_latest(ctx: &mut MongoTestContext) -> Result<(), AppError> {
        let mongodb = &ctx.state.mongodb;

        // Use timestamps far in the future so Utc::now() records don't out-sort them
        let older = ScanDocument {
            status: ScanStatus::Finished,
            timestamp: DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            details: ScanDetails::Started,
        };

        let newer = ScanDocument {
            status: ScanStatus::Running,
            timestamp: DateTime::parse_from_rfc3339("2100-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            details: ScanDetails::Started,
        };

        let id_older = mongodb.scans.insert(&older).await?;
        let id_newer = mongodb.scans.insert(&newer).await?;

        let latest = mongodb.scans.latest().await?.expect("should find a scan");

        assert_eq!(latest.timestamp, newer.timestamp);

        let _ = mongodb.scans.delete(id_older).await;
        let _ = mongodb.scans.delete(id_newer).await;

        Ok(())
    }

    /// Verifies that `mark_completed` transitions a `Running` scan to
    /// `Finished` and attaches the given counts as `ScanDetails::Completed`.
    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_mark_completed(ctx: &mut MongoTestContext) -> Result<(), AppError> {
        let mongodb = &ctx.state.mongodb;

        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now(),
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
        assert!((found.timestamp - scan.timestamp).abs() <= chrono::TimeDelta::seconds(1));

        let _ = mongodb.scans.delete(scan_id).await;

        Ok(())
    }

    /// Verifies that `mark_failed` transitions a `Running` scan to `Error`
    /// and attaches the given reason as `ScanDetails::Failed`.
    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_mark_failed(ctx: &mut MongoTestContext) -> Result<(), AppError> {
        let mongodb = &ctx.state.mongodb;

        let scan = ScanDocument {
            status: ScanStatus::Running,
            timestamp: Utc::now(),
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

        let _ = mongodb.scans.delete(scan_id).await;

        Ok(())
    }

    /// Verifies that `start` inserts a new scan in the `Running`/`Started`
    /// state with a fresh timestamp, and that it can be retrieved by the
    /// returned `ObjectId`.
    #[test_context(MongoTestContext)]
    #[test(tokio::test)]
    #[ignore = "requires mongodb test server setup"]
    async fn test_mongodb_scan_start(ctx: &mut MongoTestContext) -> Result<(), AppError> {
        let mongodb = &ctx.state.mongodb;

        let scan_id = mongodb.scans.start().await?;

        let found = mongodb
            .scans
            .find_by_id(scan_id)
            .await?
            .expect("scan should have been inserted");

        assert_eq!(found.status, ScanStatus::Running);
        assert_eq!(found.details, ScanDetails::Started);

        let _ = mongodb.scans.delete(scan_id).await;

        Ok(())
    }
}
