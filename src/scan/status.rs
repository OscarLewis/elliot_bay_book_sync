use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScanStatusMessage {
    ScanStarted {
        scan_id: Uuid,
        path: PathBuf,
    },
    ScanProgress {
        scan_id: Uuid,
        current: usize,
        total: usize,
        path: PathBuf,
    },
    ScanFinished {
        scan_id: Uuid,
        books: usize,
    },
    Error {
        message: String,
    },
}

#[derive(Clone)]
pub struct ScanStatus {
    tx: Arc<broadcast::Sender<ScanStatusMessage>>,
}

impl ScanStatus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(32);
        Self { tx: Arc::new(tx) }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ScanStatusMessage> {
        self.tx.subscribe()
    }

    pub fn send(&self, message: ScanStatusMessage) {
        let _ = self.tx.send(message);
    }
}

impl Default for ScanStatus {
    fn default() -> Self {
        Self::new()
    }
}
