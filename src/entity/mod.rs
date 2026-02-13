use crate::namespace::NamespaceRegistry;

#[cfg(test)]
mod tests;

/// Parsed entity ID with optional namespace prefix
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedEntityId {
    /// Namespace prefix (if entity ID format is "namespace/entity")
    pub namespace: Option<String>,
    /// Entity identifier
    pub entity: String,
}

/// Entity ID parsing errors
#[derive(Debug, PartialEq)]
pub enum ParseError {
    /// Empty entity ID
    Empty,
    /// Invalid format (too many slashes, empty parts)
    InvalidFormat(String),
    /// Namespace part doesn't match validation rules
    InvalidNamespace(String),
}

/// Parse entity ID into namespace and entity parts
///
/// Supports two formats:
/// - "namespace/entity" → Some("namespace"), "entity"
/// - "entity" → None, "entity"
///
/// Validates namespace part if present using namespace rules:
/// - 3-32 characters
/// - Lowercase alphanumeric + dash/underscore: [a-z0-9-_]
///
/// # Examples
///
/// ```
/// use flux::entity::parse_entity_id;
///
/// // With namespace
/// let parsed = parse_entity_id("matt/sensor-01").unwrap();
/// assert_eq!(parsed.namespace, Some("matt".to_string()));
/// assert_eq!(parsed.entity, "sensor-01".to_string());
///
/// // Without namespace (internal mode)
/// let parsed = parse_entity_id("sensor-01").unwrap();
/// assert_eq!(parsed.namespace, None);
/// assert_eq!(parsed.entity, "sensor-01".to_string());
/// ```
pub fn parse_entity_id(entity_id: &str) -> Result<ParsedEntityId, ParseError> {
    // Check for empty
    if entity_id.is_empty() {
        return Err(ParseError::Empty);
    }

    // Split on "/" delimiter
    let parts: Vec<&str> = entity_id.split('/').collect();

    match parts.len() {
        1 => {
            // Format: "entity" (no namespace)
            let entity = parts[0];
            if entity.is_empty() {
                return Err(ParseError::InvalidFormat(
                    "Entity part cannot be empty".to_string(),
                ));
            }
            Ok(ParsedEntityId {
                namespace: None,
                entity: entity.to_string(),
            })
        }
        2 => {
            // Format: "namespace/entity"
            let namespace = parts[0];
            let entity = parts[1];

            // Validate parts are not empty
            if namespace.is_empty() {
                return Err(ParseError::InvalidFormat(
                    "Namespace part cannot be empty".to_string(),
                ));
            }
            if entity.is_empty() {
                return Err(ParseError::InvalidFormat(
                    "Entity part cannot be empty".to_string(),
                ));
            }

            // Validate namespace format
            if let Err(e) = NamespaceRegistry::validate_name(namespace) {
                return Err(ParseError::InvalidNamespace(format!(
                    "Invalid namespace '{}': {:?}",
                    namespace, e
                )));
            }

            Ok(ParsedEntityId {
                namespace: Some(namespace.to_string()),
                entity: entity.to_string(),
            })
        }
        _ => {
            // Too many slashes
            Err(ParseError::InvalidFormat(format!(
                "Entity ID '{}' contains multiple '/' separators (expected at most one)",
                entity_id
            )))
        }
    }
}

/// Extract namespace from entity ID
///
/// Convenience function that returns just the namespace prefix if present.
///
/// # Examples
///
/// ```
/// use flux::entity::extract_namespace;
///
/// assert_eq!(extract_namespace("matt/sensor-01"), Some("matt".to_string()));
/// assert_eq!(extract_namespace("sensor-01"), None);
/// ```
pub fn extract_namespace(entity_id: &str) -> Option<String> {
    parse_entity_id(entity_id).ok()?.namespace
}
