use crate::state::StateEngine;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared state for query API (uses same WsAppState from websocket module)
pub struct QueryAppState {
    pub state_engine: Arc<StateEngine>,
}

/// Query parameters for entity listing
#[derive(Deserialize)]
pub struct EntityQueryParams {
    /// Filter by namespace (exact match on namespace prefix)
    pub namespace: Option<String>,
    /// Filter by entity ID prefix (string matching)
    pub prefix: Option<String>,
}

/// Entity response (matches StateEngine Entity model)
#[derive(Serialize)]
pub struct EntityResponse {
    pub id: String,
    pub properties: serde_json::Value,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Create query API router
pub fn create_query_router(state: Arc<QueryAppState>) -> Router {
    Router::new()
        .route("/api/state/entities", get(list_entities))
        .route("/api/state/entities/:id", get(get_entity))
        .with_state(state)
}

/// GET /api/state/entities - List all entities
///
/// Query parameters:
/// - `namespace`: Filter by namespace (exact match, e.g., ?namespace=matt)
/// - `prefix`: Filter by entity ID prefix (string matching, e.g., ?prefix=matt/sensor)
///
/// Both filters can be combined (AND logic):
/// - ?namespace=matt&prefix=matt/sensor
async fn list_entities(
    State(state): State<Arc<QueryAppState>>,
    Query(params): Query<EntityQueryParams>,
) -> Result<Json<Vec<EntityResponse>>, QueryError> {
    let entities = state.state_engine.get_all_entities();

    let response: Vec<EntityResponse> = entities
        .into_iter()
        .filter(|entity| {
            // Apply namespace filter if specified
            if let Some(ref namespace) = params.namespace {
                // Extract namespace from entity_id (format: "namespace/entity")
                if let Some((entity_namespace, _)) = entity.id.split_once('/') {
                    if entity_namespace != namespace {
                        return false;
                    }
                } else {
                    // Entity ID has no namespace prefix, doesn't match filter
                    return false;
                }
            }

            // Apply prefix filter if specified
            if let Some(ref prefix) = params.prefix {
                if !entity.id.starts_with(prefix) {
                    return false;
                }
            }

            true
        })
        .map(|entity| EntityResponse {
            id: entity.id,
            properties: serde_json::to_value(entity.properties)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            last_updated: entity.last_updated.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// GET /api/state/entities/:id - Get specific entity
async fn get_entity(
    State(state): State<Arc<QueryAppState>>,
    Path(id): Path<String>,
) -> Result<Json<EntityResponse>, QueryError> {
    let entity = state
        .state_engine
        .get_entity(&id)
        .ok_or(QueryError::NotFound)?;

    Ok(Json(EntityResponse {
        id: entity.id,
        properties: serde_json::to_value(entity.properties)
            .unwrap_or(serde_json::Value::Object(Default::default())),
        last_updated: entity.last_updated.to_rfc3339(),
    }))
}

/// Query error types
#[derive(Debug)]
enum QueryError {
    NotFound,
}

impl IntoResponse for QueryError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            QueryError::NotFound => (StatusCode::NOT_FOUND, "Entity not found"),
        };

        let body = Json(ErrorResponse {
            error: error_message.to_string(),
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEngine;

    fn create_test_state() -> Arc<StateEngine> {
        Arc::new(StateEngine::new())
    }

    #[tokio::test]
    async fn test_list_entities_no_filters() {
        let engine = create_test_state();
        let app_state = Arc::new(QueryAppState {
            state_engine: engine.clone(),
        });

        // Create test entities with different namespaces
        engine.update_property("matt/sensor-01", "value", serde_json::json!(42));
        engine.update_property("arc/agent-01", "value", serde_json::json!(100));
        engine.update_property("simple-entity", "value", serde_json::json!(200));

        // Query without filters - should return all entities
        let params = EntityQueryParams {
            namespace: None,
            prefix: None,
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 3);
    }

    #[tokio::test]
    async fn test_list_entities_namespace_filter() {
        let engine = create_test_state();
        let app_state = Arc::new(QueryAppState {
            state_engine: engine.clone(),
        });

        // Create test entities
        engine.update_property("matt/sensor-01", "value", serde_json::json!(42));
        engine.update_property("matt/sensor-02", "value", serde_json::json!(43));
        engine.update_property("arc/agent-01", "value", serde_json::json!(100));

        // Query with namespace filter
        let params = EntityQueryParams {
            namespace: Some("matt".to_string()),
            prefix: None,
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 2);
        assert!(result.0.iter().all(|e| e.id.starts_with("matt/")));
    }

    #[tokio::test]
    async fn test_list_entities_prefix_filter() {
        let engine = create_test_state();
        let app_state = Arc::new(QueryAppState {
            state_engine: engine.clone(),
        });

        // Create test entities
        engine.update_property("matt/sensor-01", "value", serde_json::json!(42));
        engine.update_property("matt/sensor-02", "value", serde_json::json!(43));
        engine.update_property("matt/light-01", "value", serde_json::json!(100));

        // Query with prefix filter
        let params = EntityQueryParams {
            namespace: None,
            prefix: Some("matt/sensor".to_string()),
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 2);
        assert!(result.0.iter().all(|e| e.id.starts_with("matt/sensor")));
    }

    #[tokio::test]
    async fn test_list_entities_combined_filters() {
        let engine = create_test_state();
        let app_state = Arc::new(QueryAppState {
            state_engine: engine.clone(),
        });

        // Create test entities
        engine.update_property("matt/sensor-01", "value", serde_json::json!(42));
        engine.update_property("matt/sensor-02", "value", serde_json::json!(43));
        engine.update_property("matt/light-01", "value", serde_json::json!(100));
        engine.update_property("arc/sensor-01", "value", serde_json::json!(200));

        // Query with both filters (AND logic)
        let params = EntityQueryParams {
            namespace: Some("matt".to_string()),
            prefix: Some("matt/sensor".to_string()),
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 2);
        assert!(result
            .0
            .iter()
            .all(|e| e.id.starts_with("matt/") && e.id.starts_with("matt/sensor")));
    }

    #[tokio::test]
    async fn test_list_entities_namespace_excludes_non_namespaced() {
        let engine = create_test_state();
        let app_state = Arc::new(QueryAppState {
            state_engine: engine.clone(),
        });

        // Create entities with and without namespaces
        engine.update_property("matt/sensor-01", "value", serde_json::json!(42));
        engine.update_property("simple-entity", "value", serde_json::json!(100));

        // Query with namespace filter - should exclude non-namespaced entities
        let params = EntityQueryParams {
            namespace: Some("matt".to_string()),
            prefix: None,
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0].id, "matt/sensor-01");
    }
}
