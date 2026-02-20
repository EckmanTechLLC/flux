// Integration tests for rate limiting (ADR-006 Session 3)
//
// AppState requires a live NATS connection so tests use a minimal test router
// that exercises the same rate-limit check logic as the real handlers.

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use flux::config::{new_runtime_config, RuntimeConfig, SharedRuntimeConfig};
use flux::rate_limit::RateLimiter;
use std::sync::Arc;
use tower::ServiceExt;

// ── Test state & router ───────────────────────────────────────────────────────

#[derive(Clone)]
struct RateLimitState {
    auth_enabled: bool,
    runtime_config: SharedRuntimeConfig,
    rate_limiter: Arc<RateLimiter>,
    /// Namespace to use for rate limit keying (normally extracted from entity_id)
    namespace: String,
}

async fn test_handler(
    State(s): State<RateLimitState>,
    _body: Bytes,
) -> impl IntoResponse {
    if s.auth_enabled {
        let limit = s
            .runtime_config
            .read()
            .unwrap()
            .rate_limit_per_namespace_per_minute;
        if !s.rate_limiter.check_and_consume(&s.namespace, limit) {
            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({"error": "rate limit exceeded"})),
            )
                .into_response();
            resp.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("60"),
            );
            return resp;
        }
    }
    StatusCode::OK.into_response()
}

fn create_test_app(state: RateLimitState) -> Router {
    Router::new()
        .route("/api/events", post(test_handler))
        .with_state(state)
}

fn post_request() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/events")
        .header("Content-Type", "application/json")
        .body(Body::from(b"{}".as_ref()))
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// When auth is disabled, rate limiting is a no-op — all requests pass.
#[tokio::test]
async fn test_auth_disabled_bypasses_rate_limit() {
    let rate_limiter = Arc::new(RateLimiter::new());

    // Use a tiny limit; should still pass because auth is off
    let runtime_config = new_runtime_config();
    runtime_config.write().unwrap().rate_limit_per_namespace_per_minute = 1;

    let state = RateLimitState {
        auth_enabled: false,
        runtime_config,
        rate_limiter,
        namespace: "ns1".to_string(),
    };

    // First request allowed
    let app = create_test_app(state.clone());
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second request also allowed — rate limiter is not consulted
    let app = create_test_app(state.clone());
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// When auth is enabled and within limit, requests succeed.
#[tokio::test]
async fn test_within_limit_allowed() {
    let rate_limiter = Arc::new(RateLimiter::new());
    let runtime_config = new_runtime_config();
    runtime_config.write().unwrap().rate_limit_per_namespace_per_minute = 100;

    let state = RateLimitState {
        auth_enabled: true,
        runtime_config,
        rate_limiter,
        namespace: "ns1".to_string(),
    };

    let app = create_test_app(state);
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// When auth is enabled and bucket is exhausted, returns 429 with Retry-After.
#[tokio::test]
async fn test_exceeding_limit_returns_429_with_retry_after() {
    let rate_limiter = Arc::new(RateLimiter::new());
    let runtime_config = new_runtime_config();
    // Capacity = 1 token — first request consumes it, second is blocked
    runtime_config.write().unwrap().rate_limit_per_namespace_per_minute = 1;

    let state = RateLimitState {
        auth_enabled: true,
        runtime_config,
        rate_limiter: Arc::clone(&rate_limiter),
        namespace: "ns1".to_string(),
    };

    // First request: allowed
    let app = create_test_app(state.clone());
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Second request: rate limited
    let app = create_test_app(state.clone());
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    let retry_after = resp.headers().get(axum::http::header::RETRY_AFTER);
    assert!(retry_after.is_some(), "Retry-After header must be present");
    assert_eq!(retry_after.unwrap(), "60");

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "rate limit exceeded");
}

/// Rate limits are per-namespace — one namespace's exhaustion does not affect another.
#[tokio::test]
async fn test_separate_namespaces_are_isolated() {
    let rate_limiter = Arc::new(RateLimiter::new());
    let runtime_config = new_runtime_config();
    runtime_config.write().unwrap().rate_limit_per_namespace_per_minute = 1;

    let state_ns1 = RateLimitState {
        auth_enabled: true,
        runtime_config: Arc::clone(&runtime_config),
        rate_limiter: Arc::clone(&rate_limiter),
        namespace: "ns1".to_string(),
    };
    let state_ns2 = RateLimitState {
        auth_enabled: true,
        runtime_config: Arc::clone(&runtime_config),
        rate_limiter: Arc::clone(&rate_limiter),
        namespace: "ns2".to_string(),
    };

    // Exhaust ns1
    let app = create_test_app(state_ns1.clone());
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let app = create_test_app(state_ns1);
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // ns2 is unaffected
    let app = create_test_app(state_ns2);
    let resp = app.oneshot(post_request()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// Runtime config defaults include rate_limit fields.
#[test]
fn test_runtime_config_rate_limit_defaults() {
    let cfg = RuntimeConfig::default();
    assert!(cfg.rate_limit_enabled);
    assert_eq!(cfg.rate_limit_per_namespace_per_minute, 10_000);
}
