use super::*;
use crate::state::StateEngine;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_snapshot_path_format() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: true,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 5,
    };

    let engine = Arc::new(StateEngine::new());
    let manager = SnapshotManager::new(engine.clone(), config);

    let path = manager.snapshot_path(12345);
    let filename = path.file_name().unwrap().to_str().unwrap();

    // Verify format: snapshot-{timestamp}-seq{sequence}.json.gz
    assert!(filename.starts_with("snapshot-"));
    assert!(filename.contains("-seq12345.json.gz"));
    assert!(filename.ends_with(".json.gz"));
}

#[tokio::test]
async fn test_create_and_save_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: true,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 5,
    };

    let engine = Arc::new(StateEngine::new());

    // Add some state
    engine.update_property("entity1", "temp", json!(25.5));
    engine.update_property("entity2", "status", json!("active"));

    let manager = SnapshotManager::new(engine.clone(), config);

    // Create snapshot
    manager.create_and_save_snapshot().await.unwrap();

    // Verify snapshot file exists
    let snapshots = manager.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 1);

    // Verify snapshot content
    let snapshot = Snapshot::load_from_file(&snapshots[0]).unwrap();
    assert_eq!(snapshot.entity_count(), 2);
    assert!(snapshot.entities.contains_key("entity1"));
    assert!(snapshot.entities.contains_key("entity2"));
}

#[tokio::test]
async fn test_cleanup_old_snapshots() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: true,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 3,
    };

    let engine = Arc::new(StateEngine::new());
    let manager = SnapshotManager::new(engine.clone(), config);

    // Create 5 snapshots with slight delays to ensure different timestamps
    for i in 0..5 {
        engine.update_property(&format!("entity{}", i), "value", json!(i));
        manager.create_and_save_snapshot().await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }

    // Should have kept only 3 most recent
    let snapshots = manager.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 3);
}

#[tokio::test]
async fn test_list_snapshots_filters_correctly() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: true,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 10,
    };

    let engine = Arc::new(StateEngine::new());
    let manager = SnapshotManager::new(engine.clone(), config);

    // Create a valid snapshot
    manager.create_and_save_snapshot().await.unwrap();

    // Create some files that should NOT be included
    fs::write(temp_dir.path().join("other-file.json"), "{}").unwrap();
    fs::write(temp_dir.path().join("snapshot-test.txt"), "{}").unwrap();
    fs::write(temp_dir.path().join("data.json"), "{}").unwrap();

    // Should only list the valid snapshot
    let snapshots = manager.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 1);

    let filename = snapshots[0].file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with("snapshot-"));
    assert!(filename.ends_with(".json.gz"));
}

#[tokio::test]
async fn test_disabled_manager_exits_immediately() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: false,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 5,
    };

    let engine = Arc::new(StateEngine::new());
    let manager = SnapshotManager::new(engine.clone(), config);

    // Should return immediately without error
    let result = manager.run_snapshot_loop().await;
    assert!(result.is_ok());

    // No snapshots should be created
    let snapshots = manager.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 0);
}

#[tokio::test]
async fn test_snapshot_preserves_sequence_number() {
    let temp_dir = TempDir::new().unwrap();
    let config = SnapshotConfig {
        enabled: true,
        interval_minutes: 1,
        directory: temp_dir.path().to_path_buf(),
        keep_count: 5,
    };

    let engine = Arc::new(StateEngine::new());
    engine.update_property("test", "value", json!(42));

    let manager = SnapshotManager::new(engine.clone(), config);

    // The sequence should be 0 initially
    let seq = engine.get_last_processed_sequence();
    assert_eq!(seq, 0);

    manager.create_and_save_snapshot().await.unwrap();

    // Load snapshot and verify sequence
    let snapshots = manager.list_snapshots().unwrap();
    let snapshot = Snapshot::load_from_file(&snapshots[0]).unwrap();
    assert_eq!(snapshot.sequence_number, 0);
}
