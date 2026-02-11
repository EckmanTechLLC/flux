use crate::state::StateEngine;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;

/// Shared state for query API (uses same WsAppState from websocket module)
pub struct QueryAppState {
    pub state_engine: Arc<StateEngine>,
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
async fn list_entities(
    State(state): State<Arc<QueryAppState>>,
) -> Result<Json<Vec<EntityResponse>>, QueryError> {
    let entities = state.state_engine.get_all_entities();

    let response: Vec<EntityResponse> = entities
        .into_iter()
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
