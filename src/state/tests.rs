use super::*;
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
