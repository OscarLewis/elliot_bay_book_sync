use crate::database::bson_chrono_datetime::bson_chrono_datetime;
use chrono::{DateTime, Utc};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedBookDocument {
    pub book_id: String,
    pub user_id: String,
    #[serde(rename = "_id", default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(with = "bson_chrono_datetime")]
    pub synced_at: DateTime<Utc>,
}
