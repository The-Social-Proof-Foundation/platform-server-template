use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct IndexerMetrics {
    pub checkpoints_processed: AtomicU64,
    pub events_processed: AtomicU64,
    pub events_skipped: AtomicU64,
    pub last_checkpoint_seq: Mutex<Option<String>>,
    pub last_error: Mutex<Option<String>>,
}

impl IndexerMetrics {
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "checkpointsProcessed": self.checkpoints_processed.load(Ordering::Relaxed),
            "eventsProcessed": self.events_processed.load(Ordering::Relaxed),
            "eventsSkipped": self.events_skipped.load(Ordering::Relaxed),
            "lastCheckpointSeq": self.last_checkpoint_seq.lock().ok().and_then(|g| g.clone()),
            "lastError": self.last_error.lock().ok().and_then(|g| g.clone()),
        })
    }
}

pub type SharedIndexerMetrics = Arc<IndexerMetrics>;
