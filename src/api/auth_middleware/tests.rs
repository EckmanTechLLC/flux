use super::*;
use crate::event::FluxEvent;
use axum::http::{HeaderMap, HeaderValue};
use serde_json::json;
use std::sync::Arc;

fn create_test_event(entity_id: &str) -> FluxEvent {
    FluxEvent {
        event_id: None,
        stream: "test".to_string(),
        source: "test".to_string(),
        timestamp: 1234567890,
        key: None,
        schema: None,
        payload: json!({
            "entity_id": entity_id,
            "properties": {}
        }),
    }
}

fn create_auth_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers
}

#[test]
fn test_auth_disabled_allows_all() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("test-entity");
    let headers = HeaderMap::new(); // No auth header

    // Should succeed when auth_enabled=false
    let result = authorize_event(&headers, &event, &registry, false);
    assert!(result.is_ok());
}

#[test]
fn test_auth_disabled_allows_namespaced_entity() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("matt/sensor-01");
    let headers = HeaderMap::new(); // No auth header

    // Should succeed even with namespaced entity when auth_enabled=false
    let result = authorize_event(&headers, &event, &registry, false);
    assert!(result.is_ok());
}

#[test]
fn test_auth_enabled_missing_token() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("matt/sensor-01");
    let headers = HeaderMap::new(); // No auth header

    // Should fail when auth_enabled=true and no token
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::InvalidToken(_))));
}

#[test]
fn test_auth_enabled_missing_entity_id() {
    let registry = Arc::new(NamespaceRegistry::new());
    let mut event = create_test_event("test");
    event.payload = json!({"properties": {}}); // No entity_id
    let headers = create_auth_headers("test-token");

    // Should fail when entity_id missing
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::InvalidEntityId(_))));
}

#[test]
fn test_auth_enabled_missing_namespace_prefix() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("sensor-01"); // No namespace prefix
    let headers = create_auth_headers("test-token");

    // Should fail when entity_id doesn't have namespace prefix
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::InvalidEntityId(_))));
    if let Err(AuthError::InvalidEntityId(msg)) = result {
        assert!(msg.contains("missing namespace prefix"));
    }
}

#[test]
fn test_auth_enabled_namespace_not_found() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("matt/sensor-01");
    let headers = create_auth_headers("test-token");

    // Should fail when namespace doesn't exist
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::NamespaceNotFound(_))));
}

#[test]
fn test_auth_enabled_wrong_token() {
    let registry = Arc::new(NamespaceRegistry::new());

    // Register namespace
    let ns = registry.register("matt").unwrap();
    let correct_token = ns.token;

    let event = create_test_event("matt/sensor-01");
    let headers = create_auth_headers("wrong-token");

    // Should fail when token doesn't match
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::Forbidden(_))));

    // Verify correct token would work
    let headers_correct = create_auth_headers(&correct_token);
    let result_correct = authorize_event(&headers_correct, &event, &registry, true);
    assert!(result_correct.is_ok());
}

#[test]
fn test_auth_enabled_valid_token() {
    let registry = Arc::new(NamespaceRegistry::new());

    // Register namespace
    let ns = registry.register("matt").unwrap();

    let event = create_test_event("matt/sensor-01");
    let headers = create_auth_headers(&ns.token);

    // Should succeed with correct token
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(result.is_ok());
}

#[test]
fn test_auth_enabled_different_namespace() {
    let registry = Arc::new(NamespaceRegistry::new());

    // Register two namespaces
    let ns1 = registry.register("matt").unwrap();
    let _ns2 = registry.register("alice").unwrap();

    // Try to use matt's token for alice's namespace
    let event = create_test_event("alice/sensor-01");
    let headers = create_auth_headers(&ns1.token);

    // Should fail - token owns matt, not alice
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::Forbidden(_))));
}

#[test]
fn test_auth_enabled_invalid_namespace_format() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("MATT/sensor-01"); // Uppercase not allowed
    let headers = create_auth_headers("test-token");

    // Should fail due to invalid namespace format
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::InvalidEntityId(_))));
}

#[test]
fn test_auth_enabled_multiple_slashes() {
    let registry = Arc::new(NamespaceRegistry::new());
    let event = create_test_event("matt/sensors/temperature"); // Too many slashes
    let headers = create_auth_headers("test-token");

    // Should fail due to invalid entity_id format
    let result = authorize_event(&headers, &event, &registry, true);
    assert!(matches!(result, Err(AuthError::InvalidEntityId(_))));
}
