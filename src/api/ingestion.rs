use crate::api::auth_middleware::{authorize_event, AuthError};
use crate::config::SharedRuntimeConfig;
use crate::entity::parse_entity_id;
use crate::event::FluxEvent;
use crate::namespace::NamespaceRegistry;
use crate::nats::EventPublisher;
use crate::rate_limit::RateLimiter;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
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
    pub namespace_registry: Arc<NamespaceRegistry>,
    pub auth_enabled: bool,
    pub admin_token: Option<String>,
    pub runtime_config: SharedRuntimeConfig,
    pub rate_limiter: Arc<RateLimiter>,
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
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<EventResponse>, AppError> {
    // Check body size against runtime-configurable limit
    let limit = state.runtime_config.read().unwrap().body_size_limit_single_bytes;
    if body.len() > limit {
        return Err(AppError::PayloadTooLarge);
    }

    // Deserialize from checked bytes
    let mut event: FluxEvent = serde_json::from_slice(&body)
        .map_err(|e| AppError::ValidationError(e.to_string()))?;

    // Validate and prepare event (generates UUIDv7 if needed)
    event
        .validate_and_prepare()
        .map_err(|e| AppError::ValidationError(e.to_string()))?;

    // Authorize event (if auth enabled)
    authorize_event(
        &headers,
        &event,
        &state.namespace_registry,
        state.auth_enabled,
    )?;

    // Rate limit check (auth-gated: only active when auth is enabled)
    if state.auth_enabled {
        let namespace = extract_namespace_from_event(&event);
        let limit = state
            .runtime_config
            .read()
            .unwrap()
            .rate_limit_per_namespace_per_minute;
        if !state.rate_limiter.check_and_consume(&namespace, limit) {
            return Err(AppError::RateLimited);
        }
    }

    info!(
        event_id = %event.event_id.as_ref().unwrap(),
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
        event_id: event.event_id.clone().unwrap(),
        stream: event.stream.clone(),
    }))
}

/// POST /api/events/batch - Publish multiple events
async fn publish_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<BatchResponse>, AppError> {
    // Check body size against runtime-configurable limit
    let limit = state.runtime_config.read().unwrap().body_size_limit_batch_bytes;
    if body.len() > limit {
        return Err(AppError::PayloadTooLarge);
    }

    // Deserialize from checked bytes
    let mut request: BatchRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::ValidationError(e.to_string()))?;

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

        // Authorize event (if auth enabled)
        if let Err(e) = authorize_event(
            &headers,
            event,
            &state.namespace_registry,
            state.auth_enabled,
        ) {
            failed += 1;
            results.push(BatchResult {
                event_id: event.event_id.clone(),
                stream: Some(event.stream.clone()),
                error: Some(format!("authorization failed: {}", e)),
            });
            continue;
        }

        // Rate limit check (auth-gated)
        if state.auth_enabled {
            let namespace = extract_namespace_from_event(event);
            let limit = state
                .runtime_config
                .read()
                .unwrap()
                .rate_limit_per_namespace_per_minute;
            if !state.rate_limiter.check_and_consume(&namespace, limit) {
                failed += 1;
                results.push(BatchResult {
                    event_id: event.event_id.clone(),
                    stream: Some(event.stream.clone()),
                    error: Some("rate limit exceeded".to_string()),
                });
                continue;
            }
        }

        // Publish to NATS
        match state.event_publisher.publish(event).await {
            Ok(_) => {
                successful += 1;
                results.push(BatchResult {
                    event_id: event.event_id.clone(),
                    stream: Some(event.stream.clone()),
                    error: None,
                });
            }
            Err(e) => {
                error!(error = %e, event_id = %event.event_id.as_ref().unwrap(), "Failed to publish event");
                failed += 1;
                results.push(BatchResult {
                    event_id: event.event_id.clone(),
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
    Unauthorized(String),
    Forbidden(String),
    PayloadTooLarge,
    RateLimited,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::RateLimited => {
                let body = Json(ErrorResponse {
                    error: "rate limit exceeded".to_string(),
                });
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                resp.headers_mut().insert(
                    axum::http::header::RETRY_AFTER,
                    axum::http::HeaderValue::from_static("60"),
                );
                resp
            }
            other => {
                let (status, error_message) = match other {
                    AppError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg),
                    AppError::PublishError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
                    AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
                    AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
                    AppError::PayloadTooLarge => {
                        (StatusCode::PAYLOAD_TOO_LARGE, "payload too large".to_string())
                    }
                    AppError::RateLimited => unreachable!(),
                };
                let body = Json(ErrorResponse {
                    error: error_message,
                });
                (status, body).into_response()
            }
        }
    }
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::InvalidToken(msg) => AppError::Unauthorized(msg),
            AuthError::InvalidEntityId(msg) => AppError::Unauthorized(msg),
            AuthError::NamespaceNotFound(msg) => AppError::Unauthorized(msg),
            AuthError::Forbidden(msg) => AppError::Forbidden(msg),
        }
    }
}

/// Extract namespace from event payload's entity_id, falling back to stream name.
///
/// Used for rate-limit bucket keying. If entity_id is missing or has no namespace
/// prefix, we fall back to the stream field so rate limiting still applies.
fn extract_namespace_from_event(event: &FluxEvent) -> String {
    event
        .payload
        .get("entity_id")
        .and_then(|v| v.as_str())
        .and_then(|eid| parse_entity_id(eid).ok())
        .and_then(|parsed| parsed.namespace)
        .unwrap_or_else(|| event.stream.clone())
}
