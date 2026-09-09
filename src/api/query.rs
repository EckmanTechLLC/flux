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
    /// Maximum entities to return. When set (or with `after`), results are
    /// sorted by id so paging is stable across calls.
    pub limit: Option<usize>,
    /// Cursor: return only entities whose id sorts strictly after this one.
    /// Pass the last id of the previous page.
    pub after: Option<String>,
    /// Comma-separated property names to include. Absent returns all properties.
    pub properties: Option<String>,
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

    let mut filtered: Vec<_> = entities
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
        .collect();

    // Sorting is only needed for paging, and it is not free at 60k entities —
    // so the unpaginated full-dump path keeps its previous behaviour and cost.
    let paginating = params.limit.is_some() || params.after.is_some();
    if paginating {
        filtered.sort_by(|a, b| a.id.cmp(&b.id));

        if let Some(ref after) = params.after {
            // Strictly after, so passing the previous page's last id never
            // repeats or skips an entity.
            let start = filtered.partition_point(|e| e.id.as_str() <= after.as_str());
            filtered.drain(..start);
        }
        if let Some(limit) = params.limit {
            filtered.truncate(limit);
        }
    }

    // Optional projection. Absent means every property, as before.
    let projection: Option<Vec<&str>> = params
        .properties
        .as_deref()
        .map(|p| p.split(',').map(str::trim).filter(|s| !s.is_empty()).collect());

    let response: Vec<EntityResponse> = filtered
        .into_iter()
        .map(|entity| {
            let properties = match projection {
                Some(ref keys) => {
                    let mut out = serde_json::Map::new();
                    for key in keys {
                        if let Some(v) = entity.properties.get(*key) {
                            out.insert((*key).to_string(), v.clone());
                        }
                    }
                    serde_json::Value::Object(out)
                }
                None => serde_json::to_value(entity.properties)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
            };
            EntityResponse {
                id: entity.id,
                properties,
                last_updated: entity.last_updated.to_rfc3339(),
            }
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
            limit: None,
            after: None,
            properties: None,
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
            limit: None,
            after: None,
            properties: None,
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
            limit: None,
            after: None,
            properties: None,
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
            limit: None,
            after: None,
            properties: None,
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
            limit: None,
            after: None,
            properties: None,
        };

        let result = list_entities(State(app_state), Query(params))
            .await
            .unwrap();

        assert_eq!(result.0.len(), 1);
        assert_eq!(result.0[0].id, "matt/sensor-01");
    }

    fn params(limit: Option<usize>, after: Option<&str>, props: Option<&str>) -> EntityQueryParams {
        EntityQueryParams {
            namespace: None,
            prefix: None,
            limit,
            after: after.map(str::to_string),
            properties: props.map(str::to_string),
        }
    }

    async fn seeded(n: usize) -> Arc<QueryAppState> {
        let engine = create_test_state();
        for i in 0..n {
            engine.update_property(&format!("ns/e{:03}", i), "v", serde_json::json!(i));
            engine.update_property(&format!("ns/e{:03}", i), "other", serde_json::json!("x"));
        }
        Arc::new(QueryAppState { state_engine: engine })
    }

    #[tokio::test]
    async fn limit_truncates_and_sorts() {
        let st = seeded(10).await;
        let r = list_entities(State(st), Query(params(Some(3), None, None))).await.unwrap();
        let ids: Vec<_> = r.0.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["ns/e000", "ns/e001", "ns/e002"]);
    }

    #[tokio::test]
    async fn after_is_a_stable_exclusive_cursor() {
        let st = seeded(10).await;
        let r = list_entities(State(st), Query(params(Some(3), Some("ns/e002"), None))).await.unwrap();
        let ids: Vec<_> = r.0.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["ns/e003", "ns/e004", "ns/e005"]);
    }

    #[tokio::test]
    async fn paging_covers_every_entity_exactly_once() {
        let st = seeded(25).await;
        let mut seen: Vec<String> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let p = params(Some(7), cursor.as_deref(), None);
            let page = list_entities(State(st.clone()), Query(p)).await.unwrap();
            if page.0.is_empty() { break; }
            cursor = Some(page.0.last().unwrap().id.clone());
            seen.extend(page.0.iter().map(|e| e.id.clone()));
        }
        assert_eq!(seen.len(), 25, "no entity may be skipped or repeated");
        let mut uniq = seen.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), 25);
    }

    #[tokio::test]
    async fn projection_returns_only_requested_properties() {
        let st = seeded(2).await;
        let r = list_entities(State(st), Query(params(None, None, Some("v")))).await.unwrap();
        let obj = r.0[0].properties.as_object().unwrap();
        assert!(obj.contains_key("v"));
        assert!(!obj.contains_key("other"), "unrequested properties must be dropped");
    }

    #[tokio::test]
    async fn projection_ignores_unknown_names() {
        let st = seeded(1).await;
        let r = list_entities(State(st), Query(params(None, None, Some("v, nope")))).await.unwrap();
        let obj = r.0[0].properties.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert!(obj.contains_key("v"));
    }

    #[tokio::test]
    async fn no_params_is_unchanged_full_dump() {
        let st = seeded(5).await;
        let r = list_entities(State(st), Query(params(None, None, None))).await.unwrap();
        assert_eq!(r.0.len(), 5);
        assert_eq!(r.0[0].properties.as_object().unwrap().len(), 2);
    }
}
