use crate::auth::extract_bearer_token;
use crate::entity::parse_entity_id;
use crate::event::FluxEvent;
use crate::namespace::{AuthError as NamespaceAuthError, NamespaceRegistry};
use axum::http::HeaderMap;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// Authorization errors
#[derive(Debug, PartialEq)]
pub enum AuthError {
    /// Missing or invalid Authorization header
    InvalidToken(String),
    /// Missing or invalid entity_id in payload
    InvalidEntityId(String),
    /// Namespace not found in registry
    NamespaceNotFound(String),
    /// Token doesn't own the namespace
    Forbidden(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::InvalidToken(msg) => write!(f, "Invalid token: {}", msg),
            AuthError::InvalidEntityId(msg) => write!(f, "Invalid entity ID: {}", msg),
            AuthError::NamespaceNotFound(msg) => write!(f, "Namespace not found: {}", msg),
            AuthError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
        }
    }
}

/// Authorize event ingestion
///
/// Validates that the bearer token in the request headers owns the namespace
/// extracted from the event's entity_id.
///
/// If auth is disabled, always returns Ok(()).
///
/// # Flow
/// 1. If auth_enabled=false, return Ok(())
/// 2. Extract bearer token from Authorization header
/// 3. Extract entity_id from event.payload
/// 4. Parse namespace from entity_id (using namespace/entity format)
/// 5. Validate token owns namespace
///
/// # Errors
/// - InvalidToken: Missing or malformed Authorization header
/// - InvalidEntityId: Missing entity_id or invalid namespace format
/// - NamespaceNotFound: Namespace doesn't exist in registry
/// - Forbidden: Token doesn't own the namespace
pub fn authorize_event(
    headers: &HeaderMap,
    event: &FluxEvent,
    registry: &Arc<NamespaceRegistry>,
    auth_enabled: bool,
) -> Result<(), AuthError> {
    // If auth disabled, allow all
    if !auth_enabled {
        return Ok(());
    }

    // Extract bearer token from Authorization header
    let token = extract_bearer_token(headers)
        .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

    // Extract entity_id from event payload
    let entity_id = event
        .payload
        .get("entity_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AuthError::InvalidEntityId("Missing 'entity_id' field in payload".to_string())
        })?;

    // Parse namespace from entity_id
    let parsed = parse_entity_id(entity_id).map_err(|e| {
        AuthError::InvalidEntityId(format!("Failed to parse entity_id '{}': {:?}", entity_id, e))
    })?;

    // If no namespace prefix, reject in auth mode (must use namespace/entity format)
    let namespace = parsed.namespace.ok_or_else(|| {
        AuthError::InvalidEntityId(format!(
            "Entity ID '{}' missing namespace prefix (expected 'namespace/entity' format)",
            entity_id
        ))
    })?;

    // Validate token owns namespace
    registry.validate_token(&token, &namespace).map_err(|e| {
        match e {
            NamespaceAuthError::NamespaceNotFound => {
                AuthError::NamespaceNotFound(format!("Namespace '{}' not found", namespace))
            }
            NamespaceAuthError::Unauthorized => AuthError::Forbidden(format!(
                "Token does not have permission to write to namespace '{}'",
                namespace
            )),
        }
    })?;

    Ok(())
}
