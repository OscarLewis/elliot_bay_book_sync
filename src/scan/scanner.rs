use crate::{
    AppState,
    scan::status::{ScanStatus, ScanStatusMessage},
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub scan_id: Uuid,
}

pub(crate) async fn scan_library(status: ScanStatus, scan_id: Uuid) {
    status.send(ScanStatusMessage::ScanStarted {
        scan_id,
        path: "/books".into(),
    });
}
