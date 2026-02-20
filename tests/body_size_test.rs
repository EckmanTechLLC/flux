// Integration tests for body size limit enforcement (ADR-006 Session 2)
//
// AppState requires a live NATS connection (EventPublisher), so these tests use a
// minimal test router that exercises the same body size check as the real handlers.
// The check logic is: read limit from SharedRuntimeConfig, compare body.len(), return 413.

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use flux::config::RuntimeConfig;
use tower::ServiceExt;

// ── Test router ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct BodySizeState {
    single_limit: usize,
    batch_limit: usize,
}

async fn test_single_handler(
    State(s): State<BodySizeState>,
    body: Bytes,
) -> impl IntoResponse {
    if body.len() > s.single_limit {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "payload too large"})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

async fn test_batch_handler(
    State(s): State<BodySizeState>,
    body: Bytes,
) -> impl IntoResponse {
    if body.len() > s.batch_limit {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({"error": "payload too large"})),
        )
            .into_response();
    }
    StatusCode::OK.into_response()
}

fn create_test_app(single_limit: usize, batch_limit: usize) -> Router {
    let state = BodySizeState {
        single_limit,
        batch_limit,
    };
    Router::new()
        .route("/api/events", post(test_single_handler))
        .route("/api/events/batch", post(test_batch_handler))
        .with_state(state)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// POST /api/events with body exceeding single limit → 413
#[tokio::test]
async fn test_single_event_body_too_large_returns_413() {
    let app = create_test_app(10, 10_485_760);

    let oversized = b"x".repeat(11); // 11 bytes > 10 byte limit

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("Content-Type", "application/json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "payload too large");
}

/// POST /api/events/batch with body exceeding batch limit → 413
#[tokio::test]
async fn test_batch_body_too_large_returns_413() {
    let app = create_test_app(1_048_576, 20); // batch limit = 20 bytes

    let oversized = b"x".repeat(21); // 21 bytes > 20 byte limit

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events/batch")
                .header("Content-Type", "application/json")
                .body(Body::from(oversized))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "payload too large");
}

/// POST /api/events within limit → 200 (body size check passes)
#[tokio::test]
async fn test_single_event_within_limit_passes_check() {
    let app = create_test_app(1_048_576, 10_485_760); // defaults

    let body = b"{\"key\":\"value\"}";

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .header("Content-Type", "application/json")
                .body(Body::from(body.as_ref()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Limit of exactly body.len() is allowed (boundary check)
#[tokio::test]
async fn test_body_at_exact_limit_is_allowed() {
    let body = b"x".repeat(100);
    let app = create_test_app(100, 10_485_760); // limit == body len

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/events")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// RuntimeConfig default limits are reflected correctly
#[test]
fn test_runtime_config_defaults() {
    let cfg = RuntimeConfig::default();
    assert_eq!(cfg.body_size_limit_single_bytes, 1_048_576);
    assert_eq!(cfg.body_size_limit_batch_bytes, 10_485_760);
}
