use crate::event::FluxEvent;
use crate::nats::EventPublisher;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub event_publisher: EventPublisher,
}

/// Success response for event ingestion
#[derive(Serialize)]
struct EventResponse {
    #[serde(rename = "eventId")]
    event_id: String,
    stream: String,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Batch request
#[derive(Deserialize)]
struct BatchRequest {
    events: Vec<FluxEvent>,
}

/// Batch response
#[derive(Serialize)]
struct BatchResponse {
    successful: usize,
    failed: usize,
    results: Vec<BatchResult>,
}

#[derive(Serialize)]
struct BatchResult {
    #[serde(rename = "eventId")]
    event_id: Option<String>,
    stream: Option<String>,
    error: Option<String>,
}

/// Create API router with ingestion endpoints
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/api/events", post(publish_event))
        .route("/api/events/batch", post(publish_batch))
        .with_state(Arc::new(state))
}

/// POST /api/events - Publish single event
async fn publish_event(
    State(state): State<Arc<AppState>>,
    Json(mut event): Json<FluxEvent>,
) -> Result<Json<EventResponse>, AppError> {
    // Validate and prepare event (generates UUIDv7 if needed)
    event
        .validate_and_prepare()
        .map_err(|e| AppError::ValidationError(e.to_string()))?;

    info!(
        event_id = %event.event_id,
        stream = %event.stream,
        source = %event.source,
        "Ingesting event"
    );

    // Publish to NATS
    state
        .event_publisher
        .publish(&event)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to publish event to NATS");
            AppError::PublishError(e.to_string())
        })?;

    Ok(Json(EventResponse {
        event_id: event.event_id.clone(),
        stream: event.stream.clone(),
    }))
}

/// POST /api/events/batch - Publish multiple events
async fn publish_batch(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, AppError> {
    if request.events.is_empty() {
        return Err(AppError::ValidationError(
            "Batch request must contain at least one event".to_string(),
        ));
    }

    info!(count = request.events.len(), "Ingesting event batch");

    let mut results = Vec::new();
    let mut successful = 0;
    let mut failed = 0;

    for event in &mut request.events {
        // Validate and prepare
        if let Err(e) = event.validate_and_prepare() {
            failed += 1;
            results.push(BatchResult {
                event_id: None,
                stream: Some(event.stream.clone()),
                error: Some(format!("validation failed: {}", e)),
            });
            continue;
        }

        // Publish to NATS
        match state.event_publisher.publish(event).await {
            Ok(_) => {
                successful += 1;
                results.push(BatchResult {
                    event_id: Some(event.event_id.clone()),
                    stream: Some(event.stream.clone()),
                    error: None,
                });
            }
            Err(e) => {
                error!(error = %e, event_id = %event.event_id, "Failed to publish event");
                failed += 1;
                results.push(BatchResult {
                    event_id: Some(event.event_id.clone()),
                    stream: Some(event.stream.clone()),
                    error: Some(format!("publish failed: {}", e)),
                });
            }
        }
    }

    Ok(Json(BatchResponse {
        successful,
        failed,
        results,
    }))
}

/// Application error types
enum AppError {
    ValidationError(String),
    PublishError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::PublishError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: error_message,
        });

        (status, body).into_response()
    }
}
