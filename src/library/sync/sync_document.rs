use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedBookDocument {
    pub book_id: String,
    pub user_id: String,
    pub synced_at: DateTime<Utc>,
}
