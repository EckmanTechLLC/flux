use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for snapshot manager
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Enable automatic snapshots
    pub enabled: bool,

    /// Interval between snapshots (minutes)
    pub interval_minutes: u64,

    /// Directory to store snapshots
    pub directory: PathBuf,

    /// Number of snapshots to keep (delete oldest)
    pub keep_count: usize,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 5,
            directory: PathBuf::from("/var/lib/flux/snapshots"),
            keep_count: 10,
        }
    }
}
