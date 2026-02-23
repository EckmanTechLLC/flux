use crate::api::AppState;
use crate::namespace::{RegistrationError, ValidationError};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

/// Request to register a new namespace
#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
}

/// Response for successful namespace registration
#[derive(Serialize, Deserialize)]
pub struct RegisterResponse {
    #[serde(rename = "namespaceId")]
    pub namespace_id: String,
    pub name: String,
    pub token: String,
}

/// Response for namespace lookup (NO token)
#[derive(Serialize, Deserialize)]
pub struct NamespaceInfo {
    #[serde(rename = "namespaceId")]
    pub namespace_id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "entityCount")]
    pub entity_count: u64,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Create namespace API router
pub fn create_namespace_router(state: AppState) -> Router {
    Router::new()
        .route("/api/namespaces", post(register_namespace))
        .route("/api/namespaces/:name", get(lookup_namespace))
        .with_state(Arc::new(state))
}

/// POST /api/namespaces - Register new namespace
async fn register_namespace(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, NamespaceError> {
    // Check if auth is enabled
    if !state.auth_enabled {
        return Err(NamespaceError::AuthDisabled);
    }

    // Require admin token if configured
    if let Some(ref expected) = state.admin_token {
        let provided = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if provided != Some(expected.as_str()) {
            return Err(NamespaceError::Unauthorized);
        }
    }

    info!(name = %request.name, "Registering namespace");

    // Register namespace
    let namespace = state
        .namespace_registry
        .register(&request.name)
        .map_err(NamespaceError::Registration)?;

    info!(
        namespace_id = %namespace.id,
        name = %namespace.name,
        "Namespace registered successfully"
    );

    Ok(Json(RegisterResponse {
        namespace_id: namespace.id,
        name: namespace.name,
        token: namespace.token,
    }))
}

/// GET /api/namespaces/:name - Lookup namespace (NO token in response)
async fn lookup_namespace(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<NamespaceInfo>, NamespaceError> {
    // Check if auth is enabled
    if !state.auth_enabled {
        return Err(NamespaceError::AuthDisabled);
    }

    // Look up namespace
    let namespace = state
        .namespace_registry
        .lookup_by_name(&name)
        .ok_or(NamespaceError::NotFound)?;

    Ok(Json(NamespaceInfo {
        namespace_id: namespace.id,
        name: namespace.name,
        created_at: namespace.created_at.to_rfc3339(),
        entity_count: namespace.entity_count,
    }))
}

/// Namespace API error types
enum NamespaceError {
    AuthDisabled,
    Unauthorized,
    NotFound,
    Registration(RegistrationError),
}

impl IntoResponse for NamespaceError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            NamespaceError::AuthDisabled => (
                StatusCode::NOT_FOUND,
                "Namespace registration not available (auth disabled)".to_string(),
            ),
            NamespaceError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Admin token required".to_string(),
            ),
            NamespaceError::NotFound => (
                StatusCode::NOT_FOUND,
                "Namespace not found".to_string(),
            ),
            NamespaceError::Registration(e) => match e {
                RegistrationError::InvalidName(validation_error) => {
                    let msg = match validation_error {
                        ValidationError::TooShort => {
                            "Namespace name too short (minimum 3 characters)"
                        }
                        ValidationError::TooLong => {
                            "Namespace name too long (maximum 32 characters)"
                        }
                        ValidationError::InvalidCharacters(ref detail) => detail,
                    };
                    (StatusCode::BAD_REQUEST, msg.to_string())
                }
                RegistrationError::NameAlreadyExists => (
                    StatusCode::CONFLICT,
                    "Namespace name already exists".to_string(),
                ),
                RegistrationError::StoreFailed => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to persist namespace".to_string(),
                ),
            },
        };

        let body = Json(ErrorResponse {
            error: error_message,
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::new_runtime_config;
    use crate::namespace::NamespaceRegistry;
    use crate::nats::EventPublisher;
    use crate::rate_limit::RateLimiter;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use std::sync::Arc;
    use tower::util::ServiceExt;

    async fn create_test_publisher() -> EventPublisher {
        // Create a test NATS client - won't actually connect in tests
        let client = async_nats::connect("nats://localhost:4223").await.unwrap();
        EventPublisher::new(async_nats::jetstream::new(client))
    }

    async fn create_test_app(auth_enabled: bool) -> Router {
        create_test_app_with_token(auth_enabled, None).await
    }

    async fn create_test_app_with_token(auth_enabled: bool, admin_token: Option<String>) -> Router {
        let namespace_registry = Arc::new(NamespaceRegistry::new());
        let event_publisher = create_test_publisher().await;

        let state = AppState {
            event_publisher,
            namespace_registry,
            auth_enabled,
            admin_token,
            runtime_config: new_runtime_config(),
            rate_limiter: Arc::new(RateLimiter::new()),
        };

        create_namespace_router(state)
    }

    #[tokio::test]
    async fn test_register_namespace_success() {
        let app = create_test_app(true).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: RegisterResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(response.name, "matt");
        assert!(response.namespace_id.starts_with("ns_"));
        assert!(!response.token.is_empty());
    }

    #[tokio::test]
    async fn test_register_namespace_auth_disabled() {
        let app = create_test_app(false).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_register_namespace_validation_errors() {
        let app = create_test_app(true).await;

        // Too short
        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "ab"}).to_string()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // Invalid characters
        let app2 = create_test_app(true).await;
        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "Matt@123"}).to_string()))
            .unwrap();

        let response = app2.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_register_namespace_duplicate() {
        let namespace_registry = Arc::new(NamespaceRegistry::new());

        // Create two apps with the same registry
        let event_publisher1 = create_test_publisher().await;
        let state1 = AppState {
            event_publisher: event_publisher1,
            namespace_registry: Arc::clone(&namespace_registry),
            auth_enabled: true,
            admin_token: None,
            runtime_config: new_runtime_config(),
            rate_limiter: Arc::new(RateLimiter::new()),
        };
        let app1 = create_namespace_router(state1);

        let event_publisher2 = create_test_publisher().await;
        let state2 = AppState {
            event_publisher: event_publisher2,
            namespace_registry: Arc::clone(&namespace_registry),
            auth_enabled: true,
            admin_token: None,
            runtime_config: new_runtime_config(),
            rate_limiter: Arc::new(RateLimiter::new()),
        };
        let app2 = create_namespace_router(state2);

        // Register first time
        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app1.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Try to register again with second app
        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app2.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_lookup_namespace_success() {
        let namespace_registry = Arc::new(NamespaceRegistry::new());
        namespace_registry.register("matt").unwrap();

        let event_publisher = create_test_publisher().await;

        let state = AppState {
            event_publisher,
            namespace_registry,
            auth_enabled: true,
            admin_token: None,
            runtime_config: new_runtime_config(),
            rate_limiter: Arc::new(RateLimiter::new()),
        };

        let app = create_namespace_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/namespaces/matt")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: NamespaceInfo = serde_json::from_slice(&body).unwrap();

        assert_eq!(response.name, "matt");
        assert!(response.namespace_id.starts_with("ns_"));
    }

    #[tokio::test]
    async fn test_lookup_namespace_not_found() {
        let app = create_test_app(true).await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/namespaces/nonexistent")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_lookup_namespace_auth_disabled() {
        let app = create_test_app(false).await;

        let request = Request::builder()
            .method("GET")
            .uri("/api/namespaces/matt")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_token_not_exposed_in_lookup() {
        let namespace_registry = Arc::new(NamespaceRegistry::new());
        let namespace = namespace_registry.register("matt").unwrap();
        let token = namespace.token.clone();

        let event_publisher = create_test_publisher().await;

        let state = AppState {
            event_publisher,
            namespace_registry,
            auth_enabled: true,
            admin_token: None,
            runtime_config: new_runtime_config(),
            rate_limiter: Arc::new(RateLimiter::new()),
        };

        let app = create_namespace_router(state);

        let request = Request::builder()
            .method("GET")
            .uri("/api/namespaces/matt")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Token should NOT be in the response
        assert!(!body_str.contains(&token));
    }

    #[tokio::test]
    async fn test_register_namespace_requires_admin_token() {
        let app = create_test_app_with_token(true, Some("secret".to_string())).await;

        // No Authorization header
        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_register_namespace_accepts_admin_token() {
        let app = create_test_app_with_token(true, Some("secret".to_string())).await;

        let request = Request::builder()
            .method("POST")
            .uri("/api/namespaces")
            .header("content-type", "application/json")
            .header("Authorization", "Bearer secret")
            .body(Body::from(json!({"name": "matt"}).to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
