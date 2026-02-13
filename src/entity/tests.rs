use super::*;

#[test]
fn test_parse_entity_id_with_namespace() {
    let parsed = parse_entity_id("matt/sensor-01").unwrap();
    assert_eq!(parsed.namespace, Some("matt".to_string()));
    assert_eq!(parsed.entity, "sensor-01".to_string());

    let parsed = parse_entity_id("alice_123/device-xyz").unwrap();
    assert_eq!(parsed.namespace, Some("alice_123".to_string()));
    assert_eq!(parsed.entity, "device-xyz".to_string());
}

#[test]
fn test_parse_entity_id_without_namespace() {
    let parsed = parse_entity_id("sensor-01").unwrap();
    assert_eq!(parsed.namespace, None);
    assert_eq!(parsed.entity, "sensor-01".to_string());

    let parsed = parse_entity_id("device_123").unwrap();
    assert_eq!(parsed.namespace, None);
    assert_eq!(parsed.entity, "device_123".to_string());
}

#[test]
fn test_parse_entity_id_empty() {
    let result = parse_entity_id("");
    assert!(matches!(result, Err(ParseError::Empty)));
}

#[test]
fn test_parse_entity_id_empty_namespace() {
    let result = parse_entity_id("/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidFormat(_))));
}

#[test]
fn test_parse_entity_id_empty_entity() {
    let result = parse_entity_id("matt/");
    assert!(matches!(result, Err(ParseError::InvalidFormat(_))));
}

#[test]
fn test_parse_entity_id_multiple_slashes() {
    let result = parse_entity_id("matt/sensors/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidFormat(_))));

    let result = parse_entity_id("a/b/c/d");
    assert!(matches!(result, Err(ParseError::InvalidFormat(_))));
}

#[test]
fn test_parse_entity_id_invalid_namespace_too_short() {
    let result = parse_entity_id("ab/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));
}

#[test]
fn test_parse_entity_id_invalid_namespace_too_long() {
    let long_name = "a".repeat(33);
    let entity_id = format!("{}/sensor-01", long_name);
    let result = parse_entity_id(&entity_id);
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));
}

#[test]
fn test_parse_entity_id_invalid_namespace_uppercase() {
    let result = parse_entity_id("Matt/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));
}

#[test]
fn test_parse_entity_id_invalid_namespace_special_chars() {
    let result = parse_entity_id("matt@example/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));

    let result = parse_entity_id("matt.io/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));

    let result = parse_entity_id("matt space/sensor-01");
    assert!(matches!(result, Err(ParseError::InvalidNamespace(_))));
}

#[test]
fn test_parse_entity_id_valid_namespace_chars() {
    // Valid characters: a-z, 0-9, dash, underscore
    let parsed = parse_entity_id("abc/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("abc".to_string()));

    let parsed = parse_entity_id("abc123/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("abc123".to_string()));

    let parsed = parse_entity_id("abc-def/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("abc-def".to_string()));

    let parsed = parse_entity_id("abc_def/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("abc_def".to_string()));

    let parsed = parse_entity_id("a1-2_3/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("a1-2_3".to_string()));
}

#[test]
fn test_extract_namespace_with_namespace() {
    assert_eq!(
        extract_namespace("matt/sensor-01"),
        Some("matt".to_string())
    );
    assert_eq!(
        extract_namespace("alice_123/device"),
        Some("alice_123".to_string())
    );
}

#[test]
fn test_extract_namespace_without_namespace() {
    assert_eq!(extract_namespace("sensor-01"), None);
    assert_eq!(extract_namespace("device_123"), None);
}

#[test]
fn test_extract_namespace_invalid() {
    // Returns None for invalid formats (parse failed)
    assert_eq!(extract_namespace(""), None);
    assert_eq!(extract_namespace("/sensor"), None);
    assert_eq!(extract_namespace("matt/"), None);
    assert_eq!(extract_namespace("a/b/c"), None);
    assert_eq!(extract_namespace("Matt/sensor"), None);
}

#[test]
fn test_parse_entity_id_edge_cases() {
    // Minimum valid namespace (3 chars)
    let parsed = parse_entity_id("abc/sensor").unwrap();
    assert_eq!(parsed.namespace, Some("abc".to_string()));

    // Maximum valid namespace (32 chars)
    let name_32 = "a".repeat(32);
    let entity_id = format!("{}/sensor", name_32);
    let parsed = parse_entity_id(&entity_id).unwrap();
    assert_eq!(parsed.namespace, Some(name_32));

    // Entity with special characters (no restrictions on entity part)
    let parsed = parse_entity_id("matt/sensor@01.example.com").unwrap();
    assert_eq!(parsed.namespace, Some("matt".to_string()));
    assert_eq!(parsed.entity, "sensor@01.example.com".to_string());

    // Entity with slashes is OK if escaped... wait no, can't have slashes in entity
    // because we split on slash. This is expected behavior.
}
