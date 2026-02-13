use crate::state::{Entity, StateEngine};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

pub mod config;
pub mod manager;
pub mod recovery;

#[cfg(test)]
mod tests;

/// Snapshot of world state at a specific point in time
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot format version (for future schema evolution)
    pub snapshot_version: String,

    /// Timestamp when snapshot was created
    pub created_at: DateTime<Utc>,

    /// NATS JetStream sequence number at snapshot time
    pub sequence_number: u64,

    /// All entities at snapshot time (entity_id -> Entity)
    pub entities: HashMap<String, Entity>,
}

impl Snapshot {
    /// Create snapshot from current StateEngine state
    ///
    /// # Arguments
    /// * `engine` - StateEngine to snapshot
    /// * `sequence_number` - Current NATS sequence number
    pub fn from_state_engine(engine: &StateEngine, sequence_number: u64) -> Self {
        let entities: HashMap<String, Entity> = engine
            .get_all_entities()
            .into_iter()
            .map(|entity| (entity.id.clone(), entity))
            .collect();

        Self {
            snapshot_version: "1".to_string(),
            created_at: Utc::now(),
            sequence_number,
            entities,
        }
    }

    /// Convert snapshot to HashMap for loading into StateEngine
    pub fn to_hashmap(self) -> HashMap<String, Entity> {
        self.entities
    }

    /// Save snapshot to filesystem as compressed JSON (gzip)
    ///
    /// Uses atomic write: writes to .tmp file, fsyncs, then renames.
    /// This prevents partial/corrupt snapshots from being read.
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        // Serialize to JSON
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize snapshot to JSON")?;

        // Create temporary file path
        let tmp_path = path.with_extension("tmp");

        // Write compressed JSON to temporary file
        {
            let tmp_file = File::create(&tmp_path)
                .context("Failed to create temporary snapshot file")?;

            let mut encoder = GzEncoder::new(tmp_file, Compression::default());
            encoder
                .write_all(json.as_bytes())
                .context("Failed to write compressed snapshot data")?;

            // Finish compression and get underlying file
            let file = encoder
                .finish()
                .context("Failed to finish compression")?;

            // Fsync to ensure data is written to disk
            file.sync_all()
                .context("Failed to sync snapshot file to disk")?;
        }

        // Atomically rename temp file to final path
        fs::rename(&tmp_path, path)
            .context("Failed to rename temporary snapshot file")?;

        Ok(())
    }

    /// Load snapshot from compressed JSON file (.json.gz)
    ///
    /// Supports backward compatibility: if .json.gz doesn't exist,
    /// tries loading uncompressed .json file.
    pub fn load_from_file(path: &Path) -> Result<Self> {
        // Open file for reading
        let file = File::open(path)
            .context("Failed to open snapshot file")?;

        // Check if file is gzipped by extension
        let is_compressed = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "gz")
            .unwrap_or(false);

        let snapshot = if is_compressed {
            // Decompress and deserialize
            let mut decoder = GzDecoder::new(file);
            let mut json = String::new();
            decoder
                .read_to_string(&mut json)
                .context("Failed to decompress snapshot file")?;

            serde_json::from_str(&json)
                .context("Failed to deserialize snapshot JSON")?
        } else {
            // Read uncompressed (backward compatibility)
            let mut json = String::new();
            let mut file = file;
            file.read_to_string(&mut json)
                .context("Failed to read snapshot file")?;

            serde_json::from_str(&json)
                .context("Failed to deserialize snapshot JSON")?
        };

        Ok(snapshot)
    }

    /// Get entity count (for logging/display)
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}
