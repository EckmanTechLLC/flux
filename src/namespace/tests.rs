use super::*;

#[test]
fn test_validate_name_valid() {
    // Valid names
    assert!(NamespaceRegistry::validate_name("matt").is_ok());
    assert!(NamespaceRegistry::validate_name("arc").is_ok());
    assert!(NamespaceRegistry::validate_name("sensor-team").is_ok());
    assert!(NamespaceRegistry::validate_name("test_123").is_ok());
    assert!(NamespaceRegistry::validate_name("a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6").is_ok()); // 32 chars
}

#[test]
fn test_validate_name_too_short() {
    assert_eq!(
        NamespaceRegistry::validate_name("ab"),
        Err(ValidationError::TooShort)
    );
    assert_eq!(
        NamespaceRegistry::validate_name("x"),
        Err(ValidationError::TooShort)
    );
}

#[test]
fn test_validate_name_too_long() {
    let long_name = "a".repeat(33);
    assert_eq!(
        NamespaceRegistry::validate_name(&long_name),
        Err(ValidationError::TooLong)
    );
}

#[test]
fn test_validate_name_invalid_chars() {
    // Uppercase not allowed
    let result = NamespaceRegistry::validate_name("Matt");
    assert!(matches!(result, Err(ValidationError::InvalidCharacters(_))));

    // Special characters not allowed
    let result = NamespaceRegistry::validate_name("matt.test");
    assert!(matches!(result, Err(ValidationError::InvalidCharacters(_))));

    let result = NamespaceRegistry::validate_name("matt@home");
    assert!(matches!(result, Err(ValidationError::InvalidCharacters(_))));

    let result = NamespaceRegistry::validate_name("matt test");
    assert!(matches!(result, Err(ValidationError::InvalidCharacters(_))));
}

#[test]
fn test_register_success() {
    let registry = NamespaceRegistry::new();

    // Register namespace
    let ns = registry.register("matt").expect("Registration should succeed");

    // Verify fields
    assert_eq!(ns.name, "matt");
    assert!(ns.id.starts_with("ns_"));
    assert_eq!(ns.id.len(), 11); // "ns_" + 8 chars
    assert!(!ns.token.is_empty());
    assert_eq!(ns.entity_count, 0);

    // Verify UUID v4 token format
    assert!(Uuid::parse_str(&ns.token).is_ok());
}

#[test]
fn test_register_duplicate_name() {
    let registry = NamespaceRegistry::new();

    // First registration succeeds
    registry.register("matt").expect("First registration should succeed");

    // Second registration with same name fails
    let result = registry.register("matt");
    assert_eq!(result, Err(RegistrationError::NameAlreadyExists));
}

#[test]
fn test_register_invalid_name() {
    let registry = NamespaceRegistry::new();

    // Too short
    let result = registry.register("ab");
    assert!(matches!(
        result,
        Err(RegistrationError::InvalidName(ValidationError::TooShort))
    ));

    // Invalid characters
    let result = registry.register("Matt");
    assert!(matches!(
        result,
        Err(RegistrationError::InvalidName(ValidationError::InvalidCharacters(_)))
    ));
}

#[test]
fn test_lookup_by_name() {
    let registry = NamespaceRegistry::new();

    // Register namespace
    let ns = registry.register("matt").expect("Registration should succeed");

    // Look up by name
    let found = registry
        .lookup_by_name("matt")
        .expect("Namespace should be found");
    assert_eq!(found.id, ns.id);
    assert_eq!(found.name, ns.name);
    assert_eq!(found.token, ns.token);

    // Look up non-existent
    assert!(registry.lookup_by_name("nonexistent").is_none());
}

#[test]
fn test_lookup_by_token() {
    let registry = NamespaceRegistry::new();

    // Register namespace
    let ns = registry.register("matt").expect("Registration should succeed");

    // Look up by token
    let found = registry
        .lookup_by_token(&ns.token)
        .expect("Namespace should be found");
    assert_eq!(found.id, ns.id);
    assert_eq!(found.name, ns.name);

    // Look up with wrong token
    assert!(registry.lookup_by_token("invalid-token").is_none());
}

#[test]
fn test_validate_token_success() {
    let registry = NamespaceRegistry::new();

    // Register namespace
    let ns = registry.register("matt").expect("Registration should succeed");

    // Validate correct token
    let result = registry.validate_token(&ns.token, "matt");
    assert!(result.is_ok());
}

#[test]
fn test_validate_token_wrong_token() {
    let registry = NamespaceRegistry::new();

    // Register namespace
    registry.register("matt").expect("Registration should succeed");

    // Try with wrong token
    let result = registry.validate_token("wrong-token", "matt");
    assert_eq!(result, Err(AuthError::Unauthorized));
}

#[test]
fn test_validate_token_namespace_not_found() {
    let registry = NamespaceRegistry::new();

    // Try to validate token for non-existent namespace
    let result = registry.validate_token("some-token", "nonexistent");
    assert_eq!(result, Err(AuthError::NamespaceNotFound));
}

#[test]
fn test_validate_token_cross_namespace() {
    let registry = NamespaceRegistry::new();

    // Register two namespaces
    let ns1 = registry.register("matt").expect("Registration should succeed");
    registry.register("arc").expect("Registration should succeed");

    // Try to use matt's token for arc namespace (should fail)
    let result = registry.validate_token(&ns1.token, "arc");
    assert_eq!(result, Err(AuthError::Unauthorized));
}

#[test]
fn test_count() {
    let registry = NamespaceRegistry::new();

    assert_eq!(registry.count(), 0);

    registry.register("matt").expect("Registration should succeed");
    assert_eq!(registry.count(), 1);

    registry.register("arc").expect("Registration should succeed");
    assert_eq!(registry.count(), 2);
}

#[test]
fn test_namespace_id_format() {
    let registry = NamespaceRegistry::new();

    // Register multiple namespaces and check ID format
    for name in &["matt", "arc", "test"] {
        let ns = registry.register(name).expect("Registration should succeed");
        assert!(ns.id.starts_with("ns_"));
        assert_eq!(ns.id.len(), 11);
        // Verify it's alphanumeric
        let suffix = &ns.id[3..];
        assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}

#[test]
fn test_multiple_namespaces_unique_ids() {
    let registry = NamespaceRegistry::new();

    let ns1 = registry.register("matt").expect("Registration should succeed");
    let ns2 = registry.register("arc").expect("Registration should succeed");
    let ns3 = registry.register("test").expect("Registration should succeed");

    // All IDs should be unique
    assert_ne!(ns1.id, ns2.id);
    assert_ne!(ns2.id, ns3.id);
    assert_ne!(ns1.id, ns3.id);

    // All tokens should be unique
    assert_ne!(ns1.token, ns2.token);
    assert_ne!(ns2.token, ns3.token);
    assert_ne!(ns1.token, ns3.token);
}
