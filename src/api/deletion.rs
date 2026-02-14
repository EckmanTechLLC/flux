use crate::entity::parse_entity_id;
use crate::event::FluxEvent;
use crate::namespace::NamespaceRegistry;
use crate::nats::EventPublisher;
use crate::state::StateEngine;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::delete,
    Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared state for deletion API
#[derive(Clone)]
pub struct DeletionAppState {
    pub event_publisher: EventPublisher,
    pub namespace_registry: Arc<NamespaceRegistry>,
    pub state_engine: Arc<StateEngine>,
    pub auth_enabled: bool,
    pub max_batch_delete: usize,
}

/// Response for single entity deletion
#[derive(Serialize)]
pub struct DeleteResponse {
    pub entity_id: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
}

/// Response for batch deletion
#[derive(Serialize)]
pub struct BatchDeleteResponse {
    pub deleted: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Batch delete request
#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    #[serde(flatten)]
    pub filter: DeleteFilter,
}

/// Filter for batch deletion
#[derive(Deserialize)]
#[serde(untagged)]
pub enum DeleteFilter {
    Namespace { namespace: String },
    Prefix { prefix: String },
    EntityIds { entity_ids: Vec<String> },
}

/// DELETE /api/state/entities/:id - Delete single entity
async fn delete_entity(
    State(state): State<Arc<DeletionAppState>>,
    headers: HeaderMap,
    Path(entity_id): Path<String>,
) -> Result<Json<DeleteResponse>, DeletionError> {
    // Authorize if auth is enabled
    if state.auth_enabled {
        authorize_deletion(&headers, &entity_id, &state.namespace_registry)?;
    }

    // Publish tombstone event
    let event_id = publish_tombstone(&state.event_publisher, &entity_id).await?;

    Ok(Json(DeleteResponse {
        entity_id,
        event_id,
    }))
}

/// POST /api/state/entities/delete - Batch delete entities
async fn delete_batch(
    State(state): State<Arc<DeletionAppState>>,
    headers: HeaderMap,
    Json(request): Json<BatchDeleteRequest>,
) -> Result<Json<BatchDeleteResponse>, DeletionError> {
    // Get entities matching filter
    let entities_to_delete = match &request.filter {
        DeleteFilter::Namespace { namespace } => {
            let all_entities = state.state_engine.get_all_entities();
            all_entities
                .into_iter()
                .filter(|e| e.id.starts_with(&format!("{}/", namespace)))
                .map(|e| e.id)
                .collect::<Vec<_>>()
        }
        DeleteFilter::Prefix { prefix } => {
            let all_entities = state.state_engine.get_all_entities();
            all_entities
                .into_iter()
                .filter(|e| e.id.starts_with(prefix))
                .map(|e| e.id)
                .collect::<Vec<_>>()
        }
        DeleteFilter::EntityIds { entity_ids } => entity_ids.clone(),
    };

    // Validate batch size
    if entities_to_delete.len() > state.max_batch_delete {
        return Err(DeletionError::BatchTooLarge {
            requested: entities_to_delete.len(),
            max: state.max_batch_delete,
        });
    }

    // Authorize all entities if auth is enabled
    if state.auth_enabled {
        for entity_id in &entities_to_delete {
            authorize_deletion(&headers, entity_id, &state.namespace_registry)?;
        }
    }

    // Publish tombstone events for each entity
    let mut deleted = 0;
    let mut failed = 0;
    let mut errors = Vec::new();

    for entity_id in entities_to_delete {
        match publish_tombstone(&state.event_publisher, &entity_id).await {
            Ok(_) => deleted += 1,
            Err(e) => {
                failed += 1;
                errors.push(format!("{}: {:?}", entity_id, e));
            }
        }
    }

    Ok(Json(BatchDeleteResponse {
        deleted,
        failed,
        errors,
    }))
}

/// Authorize deletion (check namespace ownership)
fn authorize_deletion(
    headers: &HeaderMap,
    entity_id: &str,
    registry: &Arc<NamespaceRegistry>,
) -> Result<(), DeletionError> {
    // Extract bearer token
    let token = extract_bearer_token(headers)?;

    // Parse entity ID to get namespace
    let parsed = parse_entity_id(entity_id)
        .map_err(|e| DeletionError::InvalidEntityId(format!("{:?}", e)))?;

    let namespace = parsed
        .namespace
        .ok_or_else(|| DeletionError::Unauthorized("Entity has no namespace".to_string()))?;

    // Validate token owns namespace
    registry
        .validate_token(&token, &namespace)
        .map_err(|_| DeletionError::Forbidden("Token does not own namespace".to_string()))?;

    Ok(())
}

/// Extract bearer token from Authorization header
fn extract_bearer_token(headers: &HeaderMap) -> Result<String, DeletionError> {
    let auth_header = headers
        .get("authorization")
        .ok_or_else(|| DeletionError::Unauthorized("Missing Authorization header".to_string()))?
        .to_str()
        .map_err(|_| DeletionError::Unauthorized("Invalid Authorization header".to_string()))?;

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(DeletionError::Unauthorized(
            "Invalid Authorization format".to_string(),
        ))
    }
}

/// Publish tombstone event to NATS
async fn publish_tombstone(
    publisher: &EventPublisher,
    entity_id: &str,
) -> Result<String, DeletionError> {
    let mut event = FluxEvent {
        event_id: None,
        stream: "flux.events.deletions".to_string(),
        source: "api".to_string(),
        timestamp: Utc::now().timestamp_millis(),
        key: Some(entity_id.to_string()),
        schema: None,
        payload: serde_json::json!({
            "entity_id": entity_id,
            "properties": {
                "__deleted__": true,
                "__deleted_at__": Utc::now().timestamp_millis()
            }
        }),
    };

    // Validate and generate event ID
    event
        .validate_and_prepare()
        .map_err(|e| DeletionError::PublishError(e.to_string()))?;

    // Publish to NATS
    publisher
        .publish(&event)
        .await
        .map_err(|e| DeletionError::PublishError(e.to_string()))?;

    Ok(event.event_id.unwrap())
}

/// Create deletion API router
pub fn create_deletion_router(state: DeletionAppState) -> Router {
    Router::new()
        .route("/api/state/entities/:id", delete(delete_entity))
        .route("/api/state/entities/delete", axum::routing::post(delete_batch))
        .with_state(Arc::new(state))
}

/// Deletion API errors
#[derive(Debug)]
pub enum DeletionError {
    Unauthorized(String),
    Forbidden(String),
    InvalidEntityId(String),
    BatchTooLarge { requested: usize, max: usize },
    PublishError(String),
}

impl IntoResponse for DeletionError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            DeletionError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            DeletionError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            DeletionError::InvalidEntityId(msg) => (StatusCode::BAD_REQUEST, msg),
            DeletionError::BatchTooLarge { requested, max } => (
                StatusCode::BAD_REQUEST,
                format!("Batch too large: {} entities requested, max is {}", requested, max),
            ),
            DeletionError::PublishError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_filter_deserialization() {
        // Test namespace filter
        let json = r#"{"namespace": "matt"}"#;
        let filter: BatchDeleteRequest = serde_json::from_str(json).unwrap();
        match filter.filter {
            DeleteFilter::Namespace { namespace } => assert_eq!(namespace, "matt"),
            _ => panic!("Expected namespace filter"),
        }

        // Test prefix filter
        let json = r#"{"prefix": "loadtest-"}"#;
        let filter: BatchDeleteRequest = serde_json::from_str(json).unwrap();
        match filter.filter {
            DeleteFilter::Prefix { prefix } => assert_eq!(prefix, "loadtest-"),
            _ => panic!("Expected prefix filter"),
        }

        // Test entity_ids filter
        let json = r#"{"entity_ids": ["id1", "id2"]}"#;
        let filter: BatchDeleteRequest = serde_json::from_str(json).unwrap();
        match filter.filter {
            DeleteFilter::EntityIds { entity_ids } => {
                assert_eq!(entity_ids, vec!["id1", "id2"])
            }
            _ => panic!("Expected entity_ids filter"),
        }
    }
}
