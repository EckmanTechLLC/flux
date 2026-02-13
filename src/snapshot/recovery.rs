use crate::snapshot::Snapshot;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

/// Load the most recent valid snapshot from directory
///
/// Returns None if no snapshots exist or all are corrupt.
/// Tries snapshots from newest to oldest until one loads successfully.
pub fn load_latest_snapshot(snapshot_dir: &Path) -> Result<Option<(Snapshot, u64)>> {
    // Create directory if it doesn't exist
    if !snapshot_dir.exists() {
        info!(
            directory = %snapshot_dir.display(),
            "Snapshot directory does not exist, starting without snapshot"
        );
        return Ok(None);
    }

    // List all snapshot files
    let mut snapshots = list_snapshots(snapshot_dir)?;

    if snapshots.is_empty() {
        info!("No snapshots found, starting from beginning");
        return Ok(None);
    }

    // Sort by filename descending (newest first, timestamp is lexicographically sortable)
    snapshots.sort_by(|a, b| b.cmp(a));

    info!(
        count = snapshots.len(),
        directory = %snapshot_dir.display(),
        "Found {} snapshot(s), attempting to load newest",
        snapshots.len()
    );

    // Try loading snapshots from newest to oldest
    for path in snapshots {
        match Snapshot::load_from_file(&path) {
            Ok(snapshot) => {
                let seq = snapshot.sequence_number;
                let entity_count = snapshot.entity_count();

                info!(
                    path = %path.display(),
                    sequence = seq,
                    entities = entity_count,
                    "Loaded snapshot successfully"
                );

                return Ok(Some((snapshot, seq)));
            }
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Corrupt snapshot, trying next oldest"
                );
                continue;
            }
        }
    }

    // All snapshots failed to load
    error!("All snapshots are corrupt, starting from beginning");
    Ok(None)
}

/// List all snapshot files in directory
fn list_snapshots(snapshot_dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(snapshot_dir).context("Failed to read snapshot directory")?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEngine;
    use tempfile::TempDir;

    #[test]
    fn test_load_latest_snapshot_no_directory() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path().join("nonexistent");

        let result = load_latest_snapshot(&snapshot_dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_latest_snapshot_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        let result = load_latest_snapshot(snapshot_dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_latest_snapshot_success() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        // Create test snapshot
        let engine = StateEngine::new();
        engine.update_property("test1", "temp", serde_json::json!(25.5));

        let snapshot = Snapshot::from_state_engine(&engine, 100);
        let path = snapshot_dir.join("snapshot-20260212T100000.000Z-seq100.json.gz");
        snapshot.save_to_file(&path).unwrap();

        // Load snapshot
        let result = load_latest_snapshot(snapshot_dir).unwrap();
        assert!(result.is_some());

        let (loaded_snapshot, seq) = result.unwrap();
        assert_eq!(seq, 100);
        assert_eq!(loaded_snapshot.entity_count(), 1);
    }

    #[test]
    fn test_load_latest_snapshot_picks_newest() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        // Create older snapshot
        let engine1 = StateEngine::new();
        engine1.update_property("test1", "value", serde_json::json!(1));
        let snapshot1 = Snapshot::from_state_engine(&engine1, 50);
        let path1 = snapshot_dir.join("snapshot-20260212T100000.000Z-seq50.json.gz");
        snapshot1.save_to_file(&path1).unwrap();

        // Create newer snapshot
        let engine2 = StateEngine::new();
        engine2.update_property("test2", "value", serde_json::json!(2));
        let snapshot2 = Snapshot::from_state_engine(&engine2, 100);
        let path2 = snapshot_dir.join("snapshot-20260212T110000.000Z-seq100.json.gz");
        snapshot2.save_to_file(&path2).unwrap();

        // Should load newest (seq 100)
        let result = load_latest_snapshot(snapshot_dir).unwrap();
        assert!(result.is_some());

        let (loaded_snapshot, seq) = result.unwrap();
        assert_eq!(seq, 100);
        assert!(loaded_snapshot.entities.contains_key("test2"));
    }

    #[test]
    fn test_load_latest_snapshot_fallback_on_corrupt() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        // Create valid older snapshot
        let engine = StateEngine::new();
        engine.update_property("test1", "value", serde_json::json!(1));
        let snapshot = Snapshot::from_state_engine(&engine, 50);
        let path1 = snapshot_dir.join("snapshot-20260212T100000.000Z-seq50.json.gz");
        snapshot.save_to_file(&path1).unwrap();

        // Create corrupt newer snapshot (invalid gzip)
        let path2 = snapshot_dir.join("snapshot-20260212T110000.000Z-seq100.json.gz");
        fs::write(&path2, b"not a gzip file").unwrap();

        // Should fall back to older valid snapshot
        let result = load_latest_snapshot(snapshot_dir).unwrap();
        assert!(result.is_some());

        let (loaded_snapshot, seq) = result.unwrap();
        assert_eq!(seq, 50);
        assert_eq!(loaded_snapshot.entity_count(), 1);
    }

    #[test]
    fn test_load_latest_snapshot_all_corrupt() {
        let temp_dir = TempDir::new().unwrap();
        let snapshot_dir = temp_dir.path();

        // Create corrupt snapshots
        let path1 = snapshot_dir.join("snapshot-20260212T100000.000Z-seq50.json.gz");
        fs::write(&path1, b"invalid gzip").unwrap();

        let path2 = snapshot_dir.join("snapshot-20260212T110000.000Z-seq100.json.gz");
        fs::write(&path2, b"not gzip at all").unwrap();

        // Should return None (cold start)
        let result = load_latest_snapshot(snapshot_dir).unwrap();
        assert!(result.is_none());
    }
}
