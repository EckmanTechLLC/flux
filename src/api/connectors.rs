//! Connector status API endpoints.
//!
//! Provides endpoints for listing and querying connector status.
//! In Phase 1, status is determined by checking if credentials exist in CredentialStore.

use crate::api::auth_middleware::AuthError;
use crate::auth::extract_bearer_token;
use crate::credentials::{CredentialStore, Credentials};
use crate::namespace::NamespaceRegistry;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[cfg(test)]
mod tests;

/// Shared application state for connector API
#[derive(Clone)]
pub struct ConnectorAppState {
    pub credential_store: Option<Arc<CredentialStore>>,
    pub namespace_registry: Arc<NamespaceRegistry>,
    pub auth_enabled: bool,
}

/// Connector status summary (for list endpoint)
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ConnectorSummary {
    pub name: String,
    pub enabled: bool,
    pub status: String,
}

/// Detailed connector status (for single connector endpoint)
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ConnectorDetail {
    pub name: String,
    pub enabled: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_poll: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub poll_interval_seconds: u64,
}

/// List connectors response
#[derive(Serialize)]
pub struct ListConnectorsResponse {
    pub connectors: Vec<ConnectorSummary>,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Request body for POST /api/connectors/:name/token
#[derive(Deserialize)]
pub struct TokenRequest {
    pub token: String,
}

/// Response for POST /api/connectors/:name/token
#[derive(Serialize)]
pub struct StoreTokenResponse {
    pub success: bool,
}

/// Response for DELETE /api/connectors/:name/token
#[derive(Serialize)]
pub struct DeleteTokenResponse {
    pub success: bool,
}

/// Available connectors (Phase 1: hardcoded from ADR-005)
const AVAILABLE_CONNECTORS: &[&str] = &["github", "gmail", "linkedin", "calendar"];

/// Create connector API router
pub fn create_connector_router(state: ConnectorAppState) -> Router {
    Router::new()
        .route("/api/connectors", get(list_connectors))
        .route("/api/connectors/:name", get(get_connector))
        .route("/api/connectors/:name/token", post(store_token))
        .route("/api/connectors/:name/token", delete(delete_token))
        .with_state(Arc::new(state))
}

/// GET /api/connectors - List all available connectors
///
/// Returns status for all connectors. If auth is enabled, only shows
/// connectors for the authenticated user's namespace.
async fn list_connectors(
    State(state): State<Arc<ConnectorAppState>>,
    headers: HeaderMap,
) -> Result<Json<ListConnectorsResponse>, AppError> {
    // Extract user namespace (if auth enabled)
    let user_namespace = if state.auth_enabled {
        let token = extract_bearer_token(&headers)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

        // Validate token and get namespace
        // For now, we extract the namespace from the token itself (it's the namespace ID)
        // In production, you might want to look this up in the registry
        Some(token)
    } else {
        None
    };

    debug!(
        namespace = ?user_namespace,
        "Listing connectors"
    );

    // If no credential store available, return all as not_configured
    let Some(credential_store) = &state.credential_store else {
        warn!("Credential store not available (FLUX_ENCRYPTION_KEY not set)");
        let connectors = AVAILABLE_CONNECTORS
            .iter()
            .map(|name| ConnectorSummary {
                name: name.to_string(),
                enabled: false,
                status: "not_configured".to_string(),
            })
            .collect();

        return Ok(Json(ListConnectorsResponse { connectors }));
    };

    // Check which connectors have credentials
    let connectors = if let Some(namespace) = user_namespace {
        // Get user's configured connectors
        let configured = credential_store
            .list_by_user(&namespace)
            .unwrap_or_else(|e| {
                warn!(error = %e, "Failed to list user connectors");
                vec![]
            });

        AVAILABLE_CONNECTORS
            .iter()
            .map(|name| {
                let has_credentials = configured.contains(&name.to_string());
                ConnectorSummary {
                    name: name.to_string(),
                    enabled: has_credentials,
                    status: if has_credentials {
                        "configured".to_string()
                    } else {
                        "not_configured".to_string()
                    },
                }
            })
            .collect()
    } else {
        // Auth disabled: check credentials under "default" namespace
        let configured = credential_store
            .list_by_user("default")
            .unwrap_or_else(|e| {
                warn!(error = %e, "Failed to list default connectors");
                vec![]
            });

        AVAILABLE_CONNECTORS
            .iter()
            .map(|name| {
                let has_credentials = configured.contains(&name.to_string());
                ConnectorSummary {
                    name: name.to_string(),
                    enabled: has_credentials,
                    status: if has_credentials {
                        "configured".to_string()
                    } else {
                        "not_configured".to_string()
                    },
                }
            })
            .collect()
    };

    Ok(Json(ListConnectorsResponse { connectors }))
}

/// GET /api/connectors/:name - Get detailed status for specific connector
///
/// Returns detailed status including poll interval and any error information.
async fn get_connector(
    State(state): State<Arc<ConnectorAppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<ConnectorDetail>, AppError> {
    // Validate connector name
    if !AVAILABLE_CONNECTORS.contains(&name.as_str()) {
        return Err(AppError::NotFound(format!(
            "Connector '{}' not found",
            name
        )));
    }

    // Extract user namespace (if auth enabled)
    let user_namespace = if state.auth_enabled {
        let token = extract_bearer_token(&headers)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;
        Some(token)
    } else {
        None
    };

    debug!(
        connector = %name,
        namespace = ?user_namespace,
        "Getting connector status"
    );

    // Default poll intervals (Phase 1: hardcoded, will come from connector config later)
    let poll_interval = match name.as_str() {
        "github" => 300,      // 5 minutes
        "gmail" => 60,        // 1 minute
        "linkedin" => 600,    // 10 minutes
        "calendar" => 300,    // 5 minutes
        _ => 300,
    };

    // Check if credentials exist
    let (enabled, status) = if let Some(credential_store) = &state.credential_store {
        if let Some(namespace) = user_namespace {
            match credential_store.get(&namespace, &name) {
                Ok(Some(_credentials)) => {
                    // Credentials exist - connector is configured
                    // Phase 1: We don't have manager integration yet, so just report "configured"
                    (true, "configured".to_string())
                }
                Ok(None) => {
                    // No credentials
                    (false, "not_configured".to_string())
                }
                Err(e) => {
                    warn!(error = %e, "Failed to fetch credentials");
                    (false, "error".to_string())
                }
            }
        } else {
            // Auth disabled: no user context
            (false, "not_configured".to_string())
        }
    } else {
        // No credential store available
        (false, "not_configured".to_string())
    };

    Ok(Json(ConnectorDetail {
        name,
        enabled,
        status,
        last_poll: None,      // Phase 1: No manager integration yet
        last_error: None,     // Phase 1: No manager integration yet
        poll_interval_seconds: poll_interval,
    }))
}

/// POST /api/connectors/:name/token - Store a PAT for a connector
///
/// Stores a personal access token as credentials. Uses "default" namespace
/// when auth is disabled, bearer token namespace when auth is enabled.
async fn store_token(
    State(state): State<Arc<ConnectorAppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<StoreTokenResponse>, AppError> {
    // Validate connector name
    if !AVAILABLE_CONNECTORS.contains(&name.as_str()) {
        return Err(AppError::NotFound(format!(
            "Connector '{}' not found",
            name
        )));
    }

    // Require credential store
    let credential_store = state.credential_store.as_ref().ok_or_else(|| {
        AppError::InternalServerError(
            "Credential storage not available (FLUX_ENCRYPTION_KEY not set)".to_string(),
        )
    })?;

    // Determine namespace
    let namespace = if state.auth_enabled {
        extract_bearer_token(&headers)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?
    } else {
        "default".to_string()
    };

    debug!(
        connector = %name,
        namespace = %namespace,
        "Storing PAT for connector"
    );

    let credentials = Credentials {
        access_token: body.token,
        refresh_token: None,
        expires_at: None,
    };

    credential_store
        .store(&namespace, &name, &credentials)
        .map_err(|e| {
            warn!(error = %e, "Failed to store credentials");
            AppError::InternalServerError("Failed to store credentials".to_string())
        })?;

    info!(
        connector = %name,
        namespace = %namespace,
        "PAT stored successfully"
    );

    Ok(Json(StoreTokenResponse { success: true }))
}

/// DELETE /api/connectors/:name/token - Remove stored credentials for a connector
///
/// Deletes the credential from the store. Returns 404 if no credential exists.
/// Uses "default" namespace when auth is disabled, bearer token namespace when enabled.
async fn delete_token(
    State(state): State<Arc<ConnectorAppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<DeleteTokenResponse>, AppError> {
    // Validate connector name
    if !AVAILABLE_CONNECTORS.contains(&name.as_str()) {
        return Err(AppError::NotFound(format!(
            "Connector '{}' not found",
            name
        )));
    }

    // Require credential store
    let credential_store = state.credential_store.as_ref().ok_or_else(|| {
        AppError::InternalServerError(
            "Credential storage not available (FLUX_ENCRYPTION_KEY not set)".to_string(),
        )
    })?;

    // Determine namespace
    let namespace = if state.auth_enabled {
        extract_bearer_token(&headers)
            .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?
    } else {
        "default".to_string()
    };

    debug!(
        connector = %name,
        namespace = %namespace,
        "Deleting token for connector"
    );

    let deleted = credential_store
        .delete(&namespace, &name)
        .map_err(|e| {
            warn!(error = %e, "Failed to delete credentials");
            AppError::InternalServerError("Failed to delete credentials".to_string())
        })?;

    if !deleted {
        return Err(AppError::NotFound(format!(
            "No credentials found for connector '{}'",
            name
        )));
    }

    info!(
        connector = %name,
        namespace = %namespace,
        "Token deleted successfully"
    );

    Ok(Json(DeleteTokenResponse { success: true }))
}

/// Application error types
enum AppError {
    Unauthorized(String),
    NotFound(String),
    InternalServerError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: error_message,
        });

        (status, body).into_response()
    }
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::InvalidToken(msg) => AppError::Unauthorized(msg),
            AuthError::InvalidEntityId(msg) => AppError::Unauthorized(msg),
            AuthError::NamespaceNotFound(msg) => AppError::Unauthorized(msg),
            AuthError::Forbidden(msg) => AppError::Unauthorized(msg),
        }
    }
}
