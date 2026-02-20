// Integration tests for GET/PUT /api/admin/config

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use flux::api::{create_admin_router, AdminAppState};
use flux::config::{new_runtime_config, RuntimeConfig};
use tower::ServiceExt;

fn create_test_app(admin_token: Option<&str>) -> Router {
    let state = AdminAppState {
        runtime_config: new_runtime_config(),
        admin_token: admin_token.map(|t| t.to_string()),
    };
    create_admin_router(state)
}

fn create_test_app_with_config(runtime_config: flux::config::SharedRuntimeConfig, admin_token: Option<&str>) -> Router {
    let state = AdminAppState {
        runtime_config,
        admin_token: admin_token.map(|t| t.to_string()),
    };
    create_admin_router(state)
}

fn bearer(token: &str) -> String {
    format!("Bearer {}", token)
}

/// GET /api/admin/config returns default values.
#[tokio::test]
async fn test_get_config_returns_defaults() {
    let app = create_test_app(None);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/admin/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let cfg: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let defaults = RuntimeConfig::default();
    assert_eq!(cfg["rate_limit_enabled"], defaults.rate_limit_enabled);
    assert_eq!(
        cfg["rate_limit_per_namespace_per_minute"],
        defaults.rate_limit_per_namespace_per_minute
    );
    assert_eq!(
        cfg["body_size_limit_single_bytes"],
        defaults.body_size_limit_single_bytes
    );
    assert_eq!(
        cfg["body_size_limit_batch_bytes"],
        defaults.body_size_limit_batch_bytes
    );
}

/// PUT /api/admin/config updates all fields and GET reflects them.
#[tokio::test]
async fn test_put_config_updates_fields() {
    let shared = new_runtime_config();
    let app = create_test_app_with_config(shared.clone(), Some("secret"));

    let body = serde_json::json!({
        "rate_limit_enabled": false,
        "rate_limit_per_namespace_per_minute": 5000,
        "body_size_limit_single_bytes": 512000,
        "body_size_limit_batch_bytes": 5120000,
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/admin/config")
                .header("Content-Type", "application/json")
                .header("Authorization", bearer("secret"))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let cfg: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();

    assert_eq!(cfg["rate_limit_enabled"], false);
    assert_eq!(cfg["rate_limit_per_namespace_per_minute"], 5000);
    assert_eq!(cfg["body_size_limit_single_bytes"], 512000);
    assert_eq!(cfg["body_size_limit_batch_bytes"], 5120000);

    // Verify the shared state was updated
    let stored = shared.read().unwrap();
    assert!(!stored.rate_limit_enabled);
    assert_eq!(stored.rate_limit_per_namespace_per_minute, 5000);
}

/// PUT /api/admin/config with wrong token returns 401.
#[tokio::test]
async fn test_put_config_wrong_token_returns_401() {
    let app = create_test_app(Some("correct-token"));

    let body = serde_json::json!({ "rate_limit_enabled": false });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/admin/config")
                .header("Content-Type", "application/json")
                .header("Authorization", bearer("wrong-token"))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// PUT /api/admin/config with no Authorization header returns 401.
#[tokio::test]
async fn test_put_config_missing_token_returns_401() {
    let app = create_test_app(Some("secret"));

    let body = serde_json::json!({ "rate_limit_enabled": false });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/admin/config")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// PUT with partial body only updates specified fields; others remain at their current values.
#[tokio::test]
async fn test_put_config_partial_update() {
    let shared = new_runtime_config();
    let app = create_test_app_with_config(shared.clone(), Some("secret"));

    // Only update rate_limit_enabled
    let body = serde_json::json!({ "rate_limit_enabled": false });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/admin/config")
                .header("Content-Type", "application/json")
                .header("Authorization", bearer("secret"))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let stored = shared.read().unwrap();
    // Changed field
    assert!(!stored.rate_limit_enabled);
    // Unchanged fields remain at defaults
    let defaults = RuntimeConfig::default();
    assert_eq!(
        stored.rate_limit_per_namespace_per_minute,
        defaults.rate_limit_per_namespace_per_minute
    );
    assert_eq!(
        stored.body_size_limit_single_bytes,
        defaults.body_size_limit_single_bytes
    );
    assert_eq!(
        stored.body_size_limit_batch_bytes,
        defaults.body_size_limit_batch_bytes
    );
}
