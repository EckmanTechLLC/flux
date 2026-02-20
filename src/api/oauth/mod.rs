//! OAuth 2.0 authorization flow for external service connections.
//!
//! Implements the authorization code flow:
//! 1. User clicks "Connect" in UI
//! 2. GET /api/connectors/:name/oauth/start → Redirect to provider
//! 3. User authorizes on provider's site
//! 4. Provider redirects to /api/connectors/:name/oauth/callback
//! 5. Exchange code for token, store encrypted credentials
//! 6. Connector is now "connected" and can poll

mod exchange;
mod provider;
mod state_manager;

pub use state_manager::{run_state_cleanup, StateManager};

use crate::auth::extract_bearer_token;
use crate::credentials::CredentialStore;
use crate::namespace::NamespaceRegistry;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Application error types for OAuth endpoints
enum AppError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    ServerError(String),
    BadGateway(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::ServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, msg),
        };

        let body = Json(ErrorResponse {
            error: error_message,
        });

        (status, body).into_response()
    }
}

/// Shared application state for OAuth API
#[derive(Clone)]
pub struct OAuthAppState {
    pub credential_store: Arc<CredentialStore>,
    pub namespace_registry: Arc<NamespaceRegistry>,
    pub state_manager: StateManager,
    pub auth_enabled: bool,
    pub callback_base_url: String,
}

/// OAuth callback query parameters
#[derive(Deserialize)]
pub struct OAuthCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// OAuth success response
#[derive(Serialize)]
pub struct OAuthSuccessResponse {
    success: bool,
    message: String,
    connector: String,
}

/// Create OAuth API router
pub fn create_oauth_router(state: OAuthAppState) -> Router {
    Router::new()
        .route("/api/connectors/:name/oauth/start", get(oauth_start))
        .route("/api/connectors/:name/oauth/callback", get(oauth_callback))
        .with_state(Arc::new(state))
}

/// GET /api/connectors/:name/oauth/start
///
/// Initiates OAuth flow by redirecting user to provider's authorization page.
///
/// # Security
/// - Requires bearer token (namespace extracted from token)
/// - Generates CSRF state parameter
/// - State stored in-memory with 10-minute expiry
async fn oauth_start(
    State(state): State<Arc<OAuthAppState>>,
    Path(connector_name): Path<String>,
    headers: HeaderMap,
) -> Result<Redirect, AppError> {
    debug!(connector = %connector_name, "OAuth start requested");

    // Validate connector name
    if !provider::is_valid_connector(&connector_name) {
        warn!(connector = %connector_name, "Invalid connector name");
        return Err(AppError::NotFound(format!(
            "Connector '{}' not found",
            connector_name
        )));
    }

    // Extract namespace from bearer token
    let namespace = if state.auth_enabled {
        extract_bearer_token(&headers)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?
    } else {
        // No auth mode: use a default namespace
        "default".to_string()
    };

    debug!(connector = %connector_name, namespace = %namespace, "User authenticated");

    // Get OAuth provider config
    let provider_config = provider::get_provider_config(&connector_name).ok_or_else(|| {
        error!(connector = %connector_name, "OAuth provider config not found (missing env vars?)");
        AppError::ServerError(format!(
            "OAuth not configured for connector '{}'. Set FLUX_OAUTH_{}_CLIENT_ID and FLUX_OAUTH_{}_CLIENT_SECRET environment variables.",
            connector_name,
            connector_name.to_uppercase(),
            connector_name.to_uppercase()
        ))
    })?;

    // Generate CSRF state parameter
    let csrf_state = state.state_manager.create_state(&connector_name, &namespace);

    // Build callback URL
    let redirect_uri = format!(
        "{}/api/connectors/{}/oauth/callback",
        state.callback_base_url, connector_name
    );

    // Build authorization URL
    let auth_url = provider_config.build_auth_url(&csrf_state, &redirect_uri);

    info!(
        connector = %connector_name,
        namespace = %namespace,
        "Redirecting to OAuth provider"
    );

    Ok(Redirect::temporary(&auth_url))
}

/// GET /api/connectors/:name/oauth/callback
///
/// OAuth callback endpoint. Exchanges authorization code for access token
/// and stores encrypted credentials.
///
/// # Security
/// - Validates CSRF state parameter
/// - Single-use state (consumed on validation)
/// - Namespace isolation (user can only connect their own accounts)
async fn oauth_callback(
    State(state): State<Arc<OAuthAppState>>,
    Path(connector_name): Path<String>,
    Query(callback): Query<OAuthCallback>,
) -> Result<Response, AppError> {
    debug!(connector = %connector_name, "OAuth callback received");

    // Check for OAuth errors
    if let Some(error) = callback.error {
        let description = callback
            .error_description
            .unwrap_or_else(|| "Unknown error".to_string());
        warn!(
            connector = %connector_name,
            error = %error,
            description = %description,
            "OAuth authorization failed"
        );
        return Err(AppError::BadRequest(format!(
            "OAuth authorization failed: {} - {}",
            error, description
        )));
    }

    // Extract code and state
    let code = callback
        .code
        .ok_or_else(|| AppError::BadRequest("Missing 'code' parameter".to_string()))?;
    let csrf_state = callback
        .state
        .ok_or_else(|| AppError::BadRequest("Missing 'state' parameter".to_string()))?;

    debug!(connector = %connector_name, state = %csrf_state, "Validating CSRF state");

    // Validate and consume CSRF state
    let state_entry = state
        .state_manager
        .validate_and_consume(&csrf_state)
        .ok_or_else(|| {
            warn!(state = %csrf_state, "Invalid or expired OAuth state");
            AppError::Unauthorized("Invalid or expired OAuth state (possible CSRF attack)".to_string())
        })?;

    // Verify connector name matches state
    if state_entry.connector != connector_name {
        error!(
            expected = %state_entry.connector,
            actual = %connector_name,
            "Connector name mismatch"
        );
        return Err(AppError::BadRequest(
            "Connector name mismatch".to_string(),
        ));
    }

    let namespace = state_entry.namespace;

    debug!(
        connector = %connector_name,
        namespace = %namespace,
        "CSRF state validated"
    );

    // Get OAuth provider config
    let provider_config = provider::get_provider_config(&connector_name).ok_or_else(|| {
        error!(connector = %connector_name, "OAuth provider config not found");
        AppError::ServerError(format!(
            "OAuth not configured for connector '{}'",
            connector_name
        ))
    })?;

    // Build redirect URI (must match the one used in start)
    let redirect_uri = format!(
        "{}/api/connectors/{}/oauth/callback",
        state.callback_base_url, connector_name
    );

    // Exchange authorization code for access token
    debug!(connector = %connector_name, "Exchanging authorization code for token");
    let credentials = exchange::exchange_code_for_token(
        &provider_config.token_url,
        &code,
        &redirect_uri,
        &provider_config.client_id,
        &provider_config.client_secret,
    )
    .await
    .map_err(|e| {
        error!(
            connector = %connector_name,
            error = %e,
            "Token exchange failed"
        );
        AppError::BadGateway(format!("Failed to exchange authorization code: {}", e))
    })?;

    // Store encrypted credentials
    debug!(
        connector = %connector_name,
        namespace = %namespace,
        "Storing encrypted credentials"
    );
    state
        .credential_store
        .store(&namespace, &connector_name, &credentials)
        .map_err(|e| {
            error!(
                connector = %connector_name,
                namespace = %namespace,
                error = %e,
                "Failed to store credentials"
            );
            AppError::ServerError(format!("Failed to store credentials: {}", e))
        })?;

    info!(
        connector = %connector_name,
        namespace = %namespace,
        has_refresh_token = credentials.refresh_token.is_some(),
        "OAuth flow completed successfully"
    );

    // Return JSON success response
    Ok(Json(OAuthSuccessResponse {
        success: true,
        message: format!("Successfully connected {}", connector_name),
        connector: connector_name,
    })
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth_callback_deserialization() {
        // Success case
        let query = "code=auth_code_123&state=csrf_state_456";
        let callback: OAuthCallback = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(callback.code, Some("auth_code_123".to_string()));
        assert_eq!(callback.state, Some("csrf_state_456".to_string()));
        assert_eq!(callback.error, None);

        // Error case
        let query = "error=access_denied&error_description=User+cancelled";
        let callback: OAuthCallback = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(callback.error, Some("access_denied".to_string()));
        assert_eq!(callback.error_description, Some("User cancelled".to_string()));
        assert_eq!(callback.code, None);
    }

    #[test]
    fn test_oauth_success_response_serialization() {
        let response = OAuthSuccessResponse {
            success: true,
            message: "Connected to GitHub".to_string(),
            connector: "github".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"connector\":\"github\""));
    }
}
