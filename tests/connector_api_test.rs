// Integration tests for connector status API

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use flux::api::{create_connector_router, ConnectorAppState};
use flux::credentials::CredentialStore;
use flux::namespace::NamespaceRegistry;
use std::sync::Arc;
use tower::ServiceExt;

fn json_body(body: &str) -> Body {
    Body::from(body.to_string())
}

fn create_test_app(with_store: bool) -> Router {
    let namespace_registry = Arc::new(NamespaceRegistry::new());

    // Optionally create credential store
    let credential_store = if with_store {
        // Generate test key
        let key = BASE64.encode(&[0u8; 32]);
        let store = CredentialStore::new(":memory:", &key).unwrap();
        Some(Arc::new(store))
    } else {
        None
    };

    let state = ConnectorAppState {
        credential_store,
        namespace_registry,
        auth_enabled: false,
    };

    create_connector_router(state)
}

#[tokio::test]
async fn test_list_connectors_no_store() {
    let app = create_test_app(false);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Should return all connectors as not_configured
    let connectors = json["connectors"].as_array().unwrap();
    assert_eq!(connectors.len(), 4);

    // Check that all are not_configured
    for connector in connectors {
        assert_eq!(connector["enabled"], false);
        assert_eq!(connector["status"], "not_configured");
    }

    // Verify expected connector names
    let names: Vec<String> = connectors
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"github".to_string()));
    assert!(names.contains(&"gmail".to_string()));
    assert!(names.contains(&"linkedin".to_string()));
    assert!(names.contains(&"calendar".to_string()));
}

#[tokio::test]
async fn test_list_connectors_with_store() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let connectors = json["connectors"].as_array().unwrap();
    assert_eq!(connectors.len(), 4);
}

#[tokio::test]
async fn test_get_connector_github() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors/github")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["name"], "github");
    assert_eq!(json["enabled"], false);
    assert_eq!(json["status"], "not_configured");
    assert_eq!(json["poll_interval_seconds"], 300);
}

#[tokio::test]
async fn test_get_connector_not_found() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors/invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Connector 'invalid' not found"));
}

#[tokio::test]
async fn test_get_connector_gmail() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors/gmail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["name"], "gmail");
    assert_eq!(json["poll_interval_seconds"], 60);
}

#[tokio::test]
async fn test_store_token_success() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connectors/github/token")
                .header("content-type", "application/json")
                .body(json_body(r#"{"token":"ghp_testtoken123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
}

#[tokio::test]
async fn test_store_token_invalid_connector() {
    let app = create_test_app(true);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connectors/invalid/token")
                .header("content-type", "application/json")
                .body(json_body(r#"{"token":"sometoken"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("Connector 'invalid' not found"));
}

#[tokio::test]
async fn test_store_token_no_credential_store() {
    // App without credential store
    let app = create_test_app(false);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connectors/github/token")
                .header("content-type", "application/json")
                .body(json_body(r#"{"token":"ghp_testtoken123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_store_token_then_list_shows_configured() {
    // App with credential store
    let namespace_registry = Arc::new(NamespaceRegistry::new());
    let key = BASE64.encode(&[0u8; 32]);
    let store = CredentialStore::new(":memory:", &key).unwrap();
    let store = Arc::new(store);

    let state = ConnectorAppState {
        credential_store: Some(Arc::clone(&store)),
        namespace_registry,
        auth_enabled: false,
    };
    let app = create_connector_router(state);

    // Store a token
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connectors/github/token")
                .header("content-type", "application/json")
                .body(json_body(r#"{"token":"ghp_testtoken123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // List should now show github as configured
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/connectors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let connectors = json["connectors"].as_array().unwrap();

    let github = connectors
        .iter()
        .find(|c| c["name"] == "github")
        .unwrap();
    assert_eq!(github["enabled"], true);
    assert_eq!(github["status"], "configured");
}

#[tokio::test]
async fn test_delete_token_success() {
    let namespace_registry = Arc::new(NamespaceRegistry::new());
    let key = BASE64.encode(&[0u8; 32]);
    let store = CredentialStore::new(":memory:", &key).unwrap();
    let store = Arc::new(store);

    let state = ConnectorAppState {
        credential_store: Some(Arc::clone(&store)),
        namespace_registry,
        auth_enabled: false,
    };
    let app = create_connector_router(state);

    // Store a token first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connectors/github/token")
                .header("content-type", "application/json")
                .body(json_body(r#"{"token":"ghp_testtoken123"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Delete it
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/connectors/github/token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
}

#[tokio::test]
async fn test_delete_token_not_found() {
    let app = create_test_app(true);

    // No token stored — should return 404
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/connectors/github/token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("No credentials found for connector 'github'"));
}

#[tokio::test]
async fn test_delete_token_no_credential_store() {
    let app = create_test_app(false);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/connectors/github/token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
