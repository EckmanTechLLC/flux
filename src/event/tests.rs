use super::*;
use serde_json::json;

#[test]
fn test_valid_event_passes_validation() {
    let mut event = FluxEvent {
        event_id: None, // Will be auto-generated
        stream: "sensors.temperature".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000, // 2024-02-11 13:00:00 UTC
        key: Some("zone1".to_string()),
        schema: Some("temp-v1".to_string()),
        payload: json!({"value": 23.5, "unit": "celsius"}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_ok());
    assert!(event.event_id.is_some()); // UUIDv7 was generated
    assert_eq!(event.event_id.unwrap().len(), 36); // UUID format
}

#[test]
fn test_missing_stream_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::MissingStream);
}

#[test]
fn test_missing_source_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::MissingSource);
}

#[test]
fn test_invalid_stream_format_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "Sensors.Temp".to_string(), // Uppercase not allowed
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    match result.unwrap_err() {
        ValidationError::InvalidStreamFormat(_) => {}
        _ => panic!("Expected InvalidStreamFormat error"),
    }
}

#[test]
fn test_invalid_timestamp_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: -1, // Negative timestamp
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::InvalidTimestamp(-1));
}

#[test]
fn test_zero_timestamp_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 0,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::InvalidTimestamp(0));
}

#[test]
fn test_payload_not_object_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!("not an object"), // String instead of object
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::PayloadNotObject);
}

#[test]
fn test_payload_array_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!([1, 2, 3]), // Array instead of object
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::PayloadNotObject);
}

#[test]
fn test_null_payload_fails() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!(null),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ValidationError::MissingPayload);
}

#[test]
fn test_uuidv7_generation() {
    let mut event1 = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let mut event2 = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 24.0}),
    };

    event1.validate_and_prepare().unwrap();
    event2.validate_and_prepare().unwrap();

    // Both should have generated IDs
    assert!(event1.event_id.is_some());
    assert!(event2.event_id.is_some());

    // IDs should be different
    assert_ne!(event1.event_id, event2.event_id);

    // IDs should be valid UUID format
    assert_eq!(event1.event_id.as_ref().unwrap().len(), 36);
    assert_eq!(event2.event_id.as_ref().unwrap().len(), 36);
}

#[test]
fn test_existing_event_id_preserved() {
    let existing_id = "01933e4b-8e6f-7890-abcd-ef1234567890";
    let mut event = FluxEvent {
        event_id: Some(existing_id.to_string()),
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    event.validate_and_prepare().unwrap();

    // Existing ID should be preserved
    assert_eq!(event.event_id.as_ref().unwrap(), existing_id);
}

#[test]
fn test_optional_fields() {
    let mut event = FluxEvent {
        event_id: None,
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None, // Optional
        schema: None, // Optional
        payload: json!({"value": 23.5}),
    };

    let result = event.validate_and_prepare();
    assert!(result.is_ok());
}

#[test]
fn test_serde_serialization() {
    let event = FluxEvent {
        event_id: Some("01933e4b-8e6f-7890-abcd-ef1234567890".to_string()),
        stream: "sensors.temperature".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: Some("zone1".to_string()),
        schema: Some("temp-v1".to_string()),
        payload: json!({"value": 23.5, "unit": "celsius"}),
    };

    let json_str = serde_json::to_string(&event).unwrap();
    assert!(json_str.contains("\"eventId\""));
    assert!(json_str.contains("sensors.temperature"));

    let deserialized: FluxEvent = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.event_id, event.event_id);
    assert_eq!(deserialized.stream, event.stream);
}

#[test]
fn test_serde_skip_none_fields() {
    let event = FluxEvent {
        event_id: Some("01933e4b-8e6f-7890-abcd-ef1234567890".to_string()),
        stream: "sensors".to_string(),
        source: "sensor-001".to_string(),
        timestamp: 1707668400000,
        key: None,
        schema: None,
        payload: json!({"value": 23.5}),
    };

    let json_str = serde_json::to_string(&event).unwrap();
    // Optional None fields should not be serialized
    assert!(!json_str.contains("\"key\""));
    assert!(!json_str.contains("\"schema\""));
}
