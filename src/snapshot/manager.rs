use crate::snapshot::{config::SnapshotConfig, Snapshot};
use crate::state::StateEngine;
use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info};

#[cfg(test)]
mod tests;

/// Manages periodic snapshots of StateEngine
pub struct SnapshotManager {
    state_engine: Arc<StateEngine>,
    config: SnapshotConfig,
}

impl SnapshotManager {
    /// Create new snapshot manager
    pub fn new(state_engine: Arc<StateEngine>, config: SnapshotConfig) -> Self {
        Self {
            state_engine,
            config,
        }
    }

    /// Run background snapshot loop
    ///
    /// Periodically creates snapshots and cleans up old ones.
    /// This function runs indefinitely until the task is cancelled.
    pub async fn run_snapshot_loop(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Snapshot manager disabled, exiting loop");
            return Ok(());
        }

        info!(
            interval_minutes = self.config.interval_minutes,
            directory = %self.config.directory.display(),
            keep_count = self.config.keep_count,
            "Starting snapshot manager"
        );

        // Create directory if it doesn't exist
        fs::create_dir_all(&self.config.directory)
            .context("Failed to create snapshot directory")?;

        let mut timer = interval(Duration::from_secs(self.config.interval_minutes * 60));

        loop {
            timer.tick().await;

            if let Err(e) = self.create_and_save_snapshot().await {
                error!(error = %e, "Failed to create snapshot");
            }
        }
    }

    /// Create snapshot and save to filesystem
    async fn create_and_save_snapshot(&self) -> Result<()> {
        let seq = self.state_engine.get_last_processed_sequence();
        let snapshot = Snapshot::from_state_engine(&self.state_engine, seq);
        let entity_count = snapshot.entity_count();

        let path = self.snapshot_path(seq);
        snapshot.save_to_file(&path)?;

        info!(
            sequence = seq,
            entities = entity_count,
            path = %path.display(),
            "Snapshot saved"
        );

        self.cleanup_old_snapshots()?;

        Ok(())
    }

    /// Generate snapshot file path with timestamp and sequence
    ///
    /// Format: snapshot-{timestamp}-seq{sequence}.json.gz
    /// Example: snapshot-20260212T153045.123Z-seq12345.json.gz
    fn snapshot_path(&self, sequence: u64) -> PathBuf {
        let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
        let filename = format!("snapshot-{}-seq{}.json.gz", timestamp, sequence);
        self.config.directory.join(filename)
    }

    /// Delete old snapshots, keeping only the most recent N
    fn cleanup_old_snapshots(&self) -> Result<()> {
        let mut snapshots = self.list_snapshots()?;

        // Keep only files to delete
        if snapshots.len() <= self.config.keep_count {
            return Ok(());
        }

        // Sort by filename (timestamp is lexicographically sortable)
        snapshots.sort();

        // Calculate how many to delete
        let delete_count = snapshots.len() - self.config.keep_count;
        let to_delete = &snapshots[..delete_count];

        for path in to_delete {
            if let Err(e) = fs::remove_file(path) {
                error!(error = %e, path = %path.display(), "Failed to delete old snapshot");
            } else {
                info!(path = %path.display(), "Deleted old snapshot");
            }
        }

        Ok(())
    }

    /// List all snapshot files in directory
    fn list_snapshots(&self) -> Result<Vec<PathBuf>> {
        let entries = fs::read_dir(&self.config.directory)
            .context("Failed to read snapshot directory")?;

        let mut snapshots = Vec::new();

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            // Include both .json.gz (current) and .json (legacy) files
            if path.is_file() {
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if filename.starts_with("snapshot-")
                        && (filename.ends_with(".json.gz") || filename.ends_with(".json"))
                    {
                        snapshots.push(path);
                    }
                }
            }
        }

        Ok(snapshots)
    }
}
