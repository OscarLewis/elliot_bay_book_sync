use crate::scan::status::{ScanStatus, ScanStatusMessage};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResponse {
    pub scan_id: Uuid,
}

pub async fn scan_library(status: ScanStatus, scan_id: Uuid, scan_dir: &Path) {
    status.send(ScanStatusMessage::ScanStarted {
        scan_id,
        path: scan_dir.to_path_buf(),
    });
}
