use super::*;
use crate::event::FluxEvent;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use std::thread;

#[test]
fn test_create_entity_and_update_property() {
    let engine = StateEngine::new();

    // Update property (creates entity)
    let update = engine.update_property("agent_001", "name", json!("Alice"));

    assert_eq!(update.entity_id, "agent_001");
    assert_eq!(update.property, "name");
    assert_eq!(update.old_value, None);
    assert_eq!(update.new_value, json!("Alice"));

    // Verify entity exists
    let entity = engine.get_entity("agent_001").unwrap();
    assert_eq!(entity.id, "agent_001");
    assert_eq!(entity.properties.get("name").unwrap(), &json!("Alice"));
}

#[test]
fn test_get_entity_after_update() {
    let engine = StateEngine::new();

    engine.update_property("sensor_42", "temperature", json!(22.5));
    engine.update_property("sensor_42", "humidity", json!(60.0));

    let entity = engine.get_entity("sensor_42").unwrap();
    assert_eq!(entity.id, "sensor_42");
    assert_eq!(entity.properties.len(), 2);
    assert_eq!(entity.properties.get("temperature").unwrap(), &json!(22.5));
    assert_eq!(entity.properties.get("humidity").unwrap(), &json!(60.0));
}

#[test]
fn test_multiple_updates_to_same_entity() {
    let engine = StateEngine::new();

    // First update
    let update1 = engine.update_property("agent_001", "status", json!("idle"));
    assert_eq!(update1.old_value, None);
    assert_eq!(update1.new_value, json!("idle"));

    // Second update (should have old_value)
    let update2 = engine.update_property("agent_001", "status", json!("active"));
    assert_eq!(update2.old_value, Some(json!("idle")));
    assert_eq!(update2.new_value, json!("active"));

    // Verify final state
    let entity = engine.get_entity("agent_001").unwrap();
    assert_eq!(entity.properties.get("status").unwrap(), &json!("active"));
}

#[test]
fn test_state_updates_broadcast_correctly() {
    let engine = StateEngine::new();
    let mut rx = engine.subscribe();

    // Update property (should broadcast)
    engine.update_property("agent_001", "name", json!("Bob"));

    // Receive broadcast
    let update = rx.try_recv().unwrap();
    assert_eq!(update.entity_id, "agent_001");
    assert_eq!(update.property, "name");
    assert_eq!(update.new_value, json!("Bob"));
}

#[test]
fn test_get_all_entities() {
    let engine = StateEngine::new();

    engine.update_property("agent_001", "name", json!("Alice"));
    engine.update_property("agent_002", "name", json!("Bob"));
    engine.update_property("sensor_42", "temp", json!(20.0));

    let entities = engine.get_all_entities();
    assert_eq!(entities.len(), 3);

    let ids: Vec<String> = entities.iter().map(|e| e.id.clone()).collect();
    assert!(ids.contains(&"agent_001".to_string()));
    assert!(ids.contains(&"agent_002".to_string()));
    assert!(ids.contains(&"sensor_42".to_string()));
}

#[test]
fn test_get_nonexistent_entity() {
    let engine = StateEngine::new();
    assert!(engine.get_entity("nonexistent").is_none());
}

#[test]
fn test_concurrent_access() {
    let engine = Arc::new(StateEngine::new());
    let mut handles = vec![];

    // Spawn 10 threads, each updating different entities
    for i in 0..10 {
        let engine_clone = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            let entity_id = format!("entity_{}", i);
            engine_clone.update_property(&entity_id, "value", json!(i));
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all entities were created
    let entities = engine.get_all_entities();
    assert_eq!(entities.len(), 10);
}

#[test]
fn test_concurrent_updates_same_entity() {
    let engine = Arc::new(StateEngine::new());
    let mut handles = vec![];

    // Spawn 10 threads, all updating the same entity with different properties
    for i in 0..10 {
        let engine_clone = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            let property = format!("prop_{}", i);
            engine_clone.update_property("shared_entity", &property, json!(i));
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify entity has all 10 properties
    let entity = engine.get_entity("shared_entity").unwrap();
    assert_eq!(entity.properties.len(), 10);
}

#[test]
fn test_initial_sequence_is_zero() {
    let engine = StateEngine::new();
    assert_eq!(engine.get_last_processed_sequence(), 0);
}

#[test]
fn test_sequence_tracking_thread_safe() {
    let engine = Arc::new(StateEngine::new());
    let mut handles = vec![];

    // Simulate sequence reads from multiple threads
    // (In reality, only run_subscriber updates sequence, but testing thread safety)
    for _ in 0..10 {
        let engine_clone = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            // Verify concurrent reads don't panic
            let _ = engine_clone.get_last_processed_sequence();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify we can still read sequence
    let seq = engine.get_last_processed_sequence();
    assert_eq!(seq, 0); // Still 0 since we only read, didn't write
}

#[test]
fn test_load_from_snapshot() {
    use std::collections::HashMap;

    let engine = StateEngine::new();

    // Create snapshot data
    let mut entities = HashMap::new();
    let mut properties = HashMap::new();
    properties.insert("temp".to_string(), json!(25.5));
    properties.insert("humidity".to_string(), json!(60.0));

    let entity = Entity {
        id: "sensor_42".to_string(),
        properties,
        last_updated: Utc::now(),
    };
    entities.insert("sensor_42".to_string(), entity);

    // Load snapshot
    engine.load_from_snapshot(entities, 100);

    // Verify entities loaded
    let loaded = engine.get_entity("sensor_42").unwrap();
    assert_eq!(loaded.id, "sensor_42");
    assert_eq!(loaded.properties.get("temp").unwrap(), &json!(25.5));
    assert_eq!(loaded.properties.get("humidity").unwrap(), &json!(60.0));

    // Verify sequence set
    assert_eq!(engine.get_last_processed_sequence(), 100);
}

#[test]
fn test_load_from_snapshot_clears_existing_state() {
    use std::collections::HashMap;

    let engine = StateEngine::new();

    // Create some initial state
    engine.update_property("old_entity", "prop", json!("old"));

    // Verify old entity exists
    assert!(engine.get_entity("old_entity").is_some());

    // Load snapshot (should clear old state)
    let mut entities = HashMap::new();
    let mut properties = HashMap::new();
    properties.insert("new_prop".to_string(), json!("new"));

    let entity = Entity {
        id: "new_entity".to_string(),
        properties,
        last_updated: Utc::now(),
    };
    entities.insert("new_entity".to_string(), entity);

    engine.load_from_snapshot(entities, 50);

    // Verify old entity is gone
    assert!(engine.get_entity("old_entity").is_none());

    // Verify new entity exists
    let loaded = engine.get_entity("new_entity").unwrap();
    assert_eq!(loaded.id, "new_entity");
    assert_eq!(loaded.properties.get("new_prop").unwrap(), &json!("new"));
}

#[test]
fn test_load_from_empty_snapshot() {
    use std::collections::HashMap;

    let engine = StateEngine::new();

    // Create some initial state
    engine.update_property("entity1", "prop", json!(1));

    // Load empty snapshot
    let entities = HashMap::new();
    engine.load_from_snapshot(entities, 0);

    // Verify state is cleared
    assert!(engine.get_entity("entity1").is_none());
    assert_eq!(engine.get_all_entities().len(), 0);
    assert_eq!(engine.get_last_processed_sequence(), 0);
}

#[test]
fn test_delete_entity() {
    let engine = StateEngine::new();

    // Create entity
    engine.update_property("test_entity", "value", json!(42));
    assert!(engine.get_entity("test_entity").is_some());

    // Delete entity
    let removed = engine.delete_entity("test_entity");
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().id, "test_entity");

    // Verify entity is gone
    assert!(engine.get_entity("test_entity").is_none());
}

#[test]
fn test_delete_nonexistent_entity() {
    let engine = StateEngine::new();

    // Delete entity that doesn't exist
    let removed = engine.delete_entity("nonexistent");
    assert!(removed.is_none());
}

#[test]
fn test_tombstone_event_deletes_entity() {
    let engine = StateEngine::new();

    // Create entity
    engine.update_property("test_entity", "value", json!(42));
    assert!(engine.get_entity("test_entity").is_some());

    // Process tombstone event
    let tombstone = FluxEvent {
        event_id: Some("test_event".to_string()),
        stream: "test".to_string(),
        source: "test".to_string(),
        timestamp: Utc::now().timestamp_millis(),
        key: Some("test_entity".to_string()),
        schema: None,
        payload: json!({
            "entity_id": "test_entity",
            "properties": {
                "__deleted__": true,
                "__deleted_at__": Utc::now().timestamp_millis()
            }
        }),
    };

    engine.process_event(&tombstone);

    // Verify entity is deleted
    assert!(engine.get_entity("test_entity").is_none());
}

#[test]
fn test_consumer_delivery_no_snapshot_resets_and_delivers_all() {
    let (should_reset, policy) = StateEngine::consumer_delivery(None);
    assert!(
        should_reset,
        "no snapshot: must reset consumer to avoid inheriting stale ack offset"
    );
    assert!(
        matches!(
            policy,
            async_nats::jetstream::consumer::DeliverPolicy::All
        ),
        "no snapshot: must deliver all events from beginning"
    );
}

#[test]
fn test_consumer_delivery_with_snapshot_resumes_from_next_sequence() {
    let (should_reset, policy) = StateEngine::consumer_delivery(Some(99));
    assert!(
        !should_reset,
        "snapshot present: reuse existing durable consumer"
    );
    assert!(
        matches!(
            policy,
            async_nats::jetstream::consumer::DeliverPolicy::ByStartSequence {
                start_sequence: 100
            }
        ),
        "snapshot at seq 99: must deliver from seq 100 (seq+1)"
    );
}

#[test]
fn test_deletion_broadcast() {
    let engine = Arc::new(StateEngine::new());

    // Subscribe to deletions
    let mut deletion_rx = engine.subscribe_deletions();

    // Create and delete entity
    engine.update_property("test_entity", "value", json!(42));
    engine.delete_entity("test_entity");

    // Receive deletion event
    let deleted = deletion_rx.try_recv().unwrap();
    assert_eq!(deleted.entity_id, "test_entity");
}
