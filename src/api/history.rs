use crate::event::FluxEvent;
use async_nats::jetstream;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use chrono::{DateTime, Duration, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

/// Shared state for history API
pub struct HistoryAppState {
    pub jetstream: jetstream::Context,
}

/// Query parameters for event history
#[derive(Deserialize)]
pub struct HistoryParams {
    /// Entity ID to fetch history for (required)
    pub entity: Option<String>,
    /// ISO 8601 start timestamp (default: 24h ago)
    pub since: Option<String>,
    /// Max events to return (default: 100, max: 500)
    pub limit: Option<usize>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Create history API router
pub fn create_history_router(state: Arc<HistoryAppState>) -> Router {
    Router::new()
        .route("/api/events", get(get_events))
        .with_state(state)
}

/// GET /api/events?entity=X&since=T&limit=N
///
/// Returns raw stored events for an entity from NATS JetStream, newest first.
async fn get_events(
    State(state): State<Arc<HistoryAppState>>,
    Query(params): Query<HistoryParams>,
) -> Response {
    // entity is required
    let entity = match params.entity {
        Some(e) => e,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "entity parameter is required".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Parse `since` or default to 24h ago
    let since: DateTime<Utc> = if let Some(s) = params.since {
        match DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "invalid `since` timestamp (expected ISO 8601)".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    } else {
        Utc::now() - Duration::hours(24)
    };

    // Clamp limit to 1..=500
    let limit = params.limit.unwrap_or(100).min(500).max(1);

    // Convert chrono timestamp to time::OffsetDateTime for NATS DeliverPolicy
    let start_time = match time::OffsetDateTime::from_unix_timestamp(since.timestamp()) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to convert start time".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Get FLUX_EVENTS stream
    let stream = match state.jetstream.get_stream("FLUX_EVENTS").await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to get FLUX_EVENTS stream for history");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to access event stream".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Create ephemeral ordered consumer starting at the requested time
    let consumer = match stream
        .create_consumer(async_nats::jetstream::consumer::pull::OrderedConfig {
            deliver_policy: async_nats::jetstream::consumer::DeliverPolicy::ByStartTime {
                start_time,
            },
            ..Default::default()
        })
        .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to create history consumer");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to create event consumer".to_string(),
                }),
            )
                .into_response();
        }
    };

    let mut messages = match consumer.messages().await {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to get message stream for history");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to read events".to_string(),
                }),
            )
                .into_response();
        }
    };

    let mut collected: Vec<FluxEvent> = Vec::new();

    // Read until 200ms idle timeout or limit reached
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_millis(200),
            messages.next(),
        )
        .await
        {
            Ok(Some(Ok(msg))) => {
                if let Ok(event) = serde_json::from_slice::<FluxEvent>(&msg.payload) {
                    if event
                        .payload
                        .get("entity_id")
                        .and_then(|v| v.as_str())
                        == Some(entity.as_str())
                    {
                        collected.push(event);
                        if collected.len() >= limit {
                            break;
                        }
                    }
                }
            }
            // Stream ended, message error, or 200ms idle — stop
            Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
        }
    }

    // Reverse to newest-first
    collected.reverse();

    Json(collected).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limit() {
        assert_eq!(None::<usize>.unwrap_or(100).min(500), 100);
    }

    #[test]
    fn test_limit_clamped_to_max() {
        assert_eq!(Some(1000usize).unwrap_or(100).min(500), 500);
    }

    #[test]
    fn test_since_parse_valid() {
        let result = DateTime::parse_from_rfc3339("2026-02-22T00:00:00Z");
        assert!(result.is_ok());
    }

    #[test]
    fn test_since_parse_invalid() {
        let result = DateTime::parse_from_rfc3339("not-a-date");
        assert!(result.is_err());
    }
}
