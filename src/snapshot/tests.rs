use super::*;
use crate::state::Entity;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_snapshot_serialize_deserialize_roundtrip() {
    // Create test snapshot
    let mut entities = HashMap::new();
    entities.insert(
        "entity_1".to_string(),
        Entity {
            id: "entity_1".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("temp".to_string(), json!(22.5));
                props.insert("status".to_string(), json!("active"));
                props
            },
            last_updated: Utc::now(),
        },
    );
    entities.insert(
        "entity_2".to_string(),
        Entity {
            id: "entity_2".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("count".to_string(), json!(42));
                props
            },
            last_updated: Utc::now(),
        },
    );

    let original = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 12345,
        entities,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&original).expect("Serialization failed");

    // Deserialize back
    let deserialized: Snapshot =
        serde_json::from_str(&json).expect("Deserialization failed");

    // Verify fields match
    assert_eq!(deserialized.snapshot_version, "1");
    assert_eq!(deserialized.sequence_number, 12345);
    assert_eq!(deserialized.entities.len(), 2);
    assert!(deserialized.entities.contains_key("entity_1"));
    assert!(deserialized.entities.contains_key("entity_2"));

    // Verify entity data
    let entity_1 = &deserialized.entities["entity_1"];
    assert_eq!(entity_1.id, "entity_1");
    assert_eq!(entity_1.properties["temp"], json!(22.5));
    assert_eq!(entity_1.properties["status"], json!("active"));
}

#[test]
fn test_snapshot_save_and_load() {
    // Create test snapshot
    let mut entities = HashMap::new();
    entities.insert(
        "sensor_01".to_string(),
        Entity {
            id: "sensor_01".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("temp".to_string(), json!(23.5));
                props.insert("humidity".to_string(), json!(60.0));
                props
            },
            last_updated: Utc::now(),
        },
    );

    let original = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 999,
        entities,
    };

    // Create temp directory for test
    let temp_dir = std::env::temp_dir();
    let snapshot_path = temp_dir.join("test_snapshot.json.gz");

    // Clean up any existing test file
    let _ = std::fs::remove_file(&snapshot_path);

    // Save snapshot
    original
        .save_to_file(&snapshot_path)
        .expect("Failed to save snapshot");

    // Verify file exists
    assert!(snapshot_path.exists());

    // Load snapshot
    let loaded = Snapshot::load_from_file(&snapshot_path).expect("Failed to load snapshot");

    // Verify data matches
    assert_eq!(loaded.snapshot_version, "1");
    assert_eq!(loaded.sequence_number, 999);
    assert_eq!(loaded.entities.len(), 1);
    assert!(loaded.entities.contains_key("sensor_01"));

    let sensor = &loaded.entities["sensor_01"];
    assert_eq!(sensor.properties["temp"], json!(23.5));
    assert_eq!(sensor.properties["humidity"], json!(60.0));

    // Clean up
    std::fs::remove_file(&snapshot_path).expect("Failed to clean up test file");
}

#[test]
fn test_load_missing_file() {
    let temp_dir = std::env::temp_dir();
    let missing_path = temp_dir.join("nonexistent_snapshot.json.gz");

    // Ensure file doesn't exist
    let _ = std::fs::remove_file(&missing_path);

    // Attempt to load should fail
    let result = Snapshot::load_from_file(&missing_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to open"));
}

#[test]
fn test_load_invalid_gzip() {
    let temp_dir = std::env::temp_dir();
    let invalid_path = temp_dir.join("invalid_snapshot.json.gz");

    // Write invalid gzip data
    std::fs::write(&invalid_path, b"not a gzip file").expect("Failed to write test file");

    // Attempt to load should fail
    let result = Snapshot::load_from_file(&invalid_path);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to decompress"));

    // Clean up
    std::fs::remove_file(&invalid_path).expect("Failed to clean up test file");
}

#[test]
fn test_snapshot_from_state_engine() {
    // Create StateEngine with test data
    let engine = StateEngine::new();

    // Add test entities via update_property
    engine.update_property("agent_1", "status", json!("active"));
    engine.update_property("agent_1", "score", json!(100));
    engine.update_property("agent_2", "status", json!("idle"));

    // Create snapshot
    let snapshot = Snapshot::from_state_engine(&engine, 5000);

    // Verify snapshot
    assert_eq!(snapshot.snapshot_version, "1");
    assert_eq!(snapshot.sequence_number, 5000);
    assert_eq!(snapshot.entities.len(), 2);
    assert!(snapshot.entities.contains_key("agent_1"));
    assert!(snapshot.entities.contains_key("agent_2"));

    // Verify entity data
    let agent_1 = &snapshot.entities["agent_1"];
    assert_eq!(agent_1.properties["status"], json!("active"));
    assert_eq!(agent_1.properties["score"], json!(100));

    let agent_2 = &snapshot.entities["agent_2"];
    assert_eq!(agent_2.properties["status"], json!("idle"));
}

#[test]
fn test_snapshot_to_hashmap() {
    let mut entities = HashMap::new();
    entities.insert(
        "test_entity".to_string(),
        Entity {
            id: "test_entity".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("value".to_string(), json!(42));
                props
            },
            last_updated: Utc::now(),
        },
    );

    let snapshot = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 100,
        entities: entities.clone(),
    };

    // Convert to hashmap
    let hashmap = snapshot.to_hashmap();

    // Verify same data
    assert_eq!(hashmap.len(), 1);
    assert!(hashmap.contains_key("test_entity"));
    assert_eq!(hashmap["test_entity"].properties["value"], json!(42));
}

#[test]
fn test_snapshot_entity_count() {
    let mut entities = HashMap::new();
    for i in 0..10 {
        entities.insert(
            format!("entity_{}", i),
            Entity {
                id: format!("entity_{}", i),
                properties: HashMap::new(),
                last_updated: Utc::now(),
            },
        );
    }

    let snapshot = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 1000,
        entities,
    };

    assert_eq!(snapshot.entity_count(), 10);
}

#[test]
fn test_compression_reduces_size() {
    // Create snapshot with compressible data
    let mut entities = HashMap::new();
    for i in 0..100 {
        let mut props = HashMap::new();
        props.insert("status".to_string(), json!("active"));
        props.insert("value".to_string(), json!(i));
        props.insert("description".to_string(), json!("This is a test entity with repeating data"));

        entities.insert(
            format!("entity_{}", i),
            Entity {
                id: format!("entity_{}", i),
                properties: props,
                last_updated: Utc::now(),
            },
        );
    }

    let snapshot = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 5000,
        entities,
    };

    let temp_dir = std::env::temp_dir();
    let compressed_path = temp_dir.join("test_compressed.json.gz");
    let uncompressed_path = temp_dir.join("test_uncompressed.json");

    // Clean up any existing files
    let _ = std::fs::remove_file(&compressed_path);
    let _ = std::fs::remove_file(&uncompressed_path);

    // Save compressed
    snapshot
        .save_to_file(&compressed_path)
        .expect("Failed to save compressed snapshot");

    // Save uncompressed for comparison
    let json = serde_json::to_string_pretty(&snapshot).expect("Failed to serialize");
    std::fs::write(&uncompressed_path, json).expect("Failed to write uncompressed");

    // Verify compressed size is smaller
    let compressed_size = std::fs::metadata(&compressed_path)
        .expect("Failed to get compressed file metadata")
        .len();
    let uncompressed_size = std::fs::metadata(&uncompressed_path)
        .expect("Failed to get uncompressed file metadata")
        .len();

    assert!(
        compressed_size < uncompressed_size,
        "Compressed size ({}) should be smaller than uncompressed ({})",
        compressed_size,
        uncompressed_size
    );

    // Clean up
    std::fs::remove_file(&compressed_path).expect("Failed to clean up compressed file");
    std::fs::remove_file(&uncompressed_path).expect("Failed to clean up uncompressed file");
}

#[test]
fn test_atomic_write_no_tmp_file() {
    // Create test snapshot
    let mut entities = HashMap::new();
    entities.insert(
        "test".to_string(),
        Entity {
            id: "test".to_string(),
            properties: HashMap::new(),
            last_updated: Utc::now(),
        },
    );

    let snapshot = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 100,
        entities,
    };

    let temp_dir = std::env::temp_dir();
    let snapshot_path = temp_dir.join("test_atomic.json.gz");
    let tmp_path = temp_dir.join("test_atomic.tmp");

    // Clean up any existing files
    let _ = std::fs::remove_file(&snapshot_path);
    let _ = std::fs::remove_file(&tmp_path);

    // Save snapshot
    snapshot
        .save_to_file(&snapshot_path)
        .expect("Failed to save snapshot");

    // Verify final file exists
    assert!(snapshot_path.exists());

    // Verify .tmp file was cleaned up
    assert!(
        !tmp_path.exists(),
        "Temporary file should not exist after successful write"
    );

    // Clean up
    std::fs::remove_file(&snapshot_path).expect("Failed to clean up test file");
}

#[test]
fn test_backward_compatibility_load_uncompressed() {
    // Create test snapshot
    let mut entities = HashMap::new();
    entities.insert(
        "legacy_entity".to_string(),
        Entity {
            id: "legacy_entity".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("type".to_string(), json!("legacy"));
                props
            },
            last_updated: Utc::now(),
        },
    );

    let snapshot = Snapshot {
        snapshot_version: "1".to_string(),
        created_at: Utc::now(),
        sequence_number: 777,
        entities,
    };

    let temp_dir = std::env::temp_dir();
    let legacy_path = temp_dir.join("test_legacy.json");

    // Clean up any existing file
    let _ = std::fs::remove_file(&legacy_path);

    // Write uncompressed JSON (simulate old format)
    let json = serde_json::to_string_pretty(&snapshot).expect("Failed to serialize");
    std::fs::write(&legacy_path, json).expect("Failed to write legacy file");

    // Load using new loader (should handle uncompressed)
    let loaded = Snapshot::load_from_file(&legacy_path)
        .expect("Failed to load legacy uncompressed snapshot");

    // Verify data matches
    assert_eq!(loaded.snapshot_version, "1");
    assert_eq!(loaded.sequence_number, 777);
    assert_eq!(loaded.entities.len(), 1);
    assert!(loaded.entities.contains_key("legacy_entity"));

    // Clean up
    std::fs::remove_file(&legacy_path).expect("Failed to clean up test file");
}
