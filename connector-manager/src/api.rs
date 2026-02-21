//! Connector Manager HTTP API — generic connector endpoints.
//!
//! Exposes three routes:
//! - `POST /api/connectors/generic` — create a new generic (Bento) source
//! - `DELETE /api/connectors/generic/:source_id` — remove a generic source
//! - `GET /api/connectors` — list all connectors (builtin + generic)

use crate::generic_config::{AuthType, GenericConfigStore, GenericSourceConfig};
use crate::registry::get_all_connectors;
use crate::runners::generic::GenericRunner;
use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use chrono::Utc;
use flux::credentials::{CredentialStore, Credentials};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Shared state for the connector API handlers.
#[derive(Clone)]
pub struct ApiState {
    pub config_store: Arc<GenericConfigStore>,
    pub runner: Arc<GenericRunner>,
    pub credential_store: Arc<CredentialStore>,
}

/// Auth type as received in the API request body.
///
/// Matches the format described in ADR-007:
/// - `"none"` or `"bearer"` as a plain string
/// - `{ "api_key_header": "<header-name>" }` as an object
#[derive(Deserialize)]
#[serde(untagged)]
pub enum AuthTypeInput {
    /// Plain string: `"none"` or `"bearer"`
    Plain(String),
    /// API key via custom header: `{ "api_key_header": "X-API-Key" }`
    ApiKey { api_key_header: String },
}

impl From<AuthTypeInput> for AuthType {
    fn from(input: AuthTypeInput) -> Self {
        match input {
            AuthTypeInput::Plain(s) if s == "bearer" => AuthType::BearerToken,
            AuthTypeInput::Plain(_) => AuthType::None,
            AuthTypeInput::ApiKey { api_key_header } => AuthType::ApiKeyHeader {
                header_name: api_key_header,
            },
        }
    }
}

/// Request body for `POST /api/connectors/generic`.
#[derive(Deserialize)]
pub struct CreateGenericSourceRequest {
    pub name: String,
    pub url: String,
    pub poll_interval_secs: u64,
    pub entity_key: String,
    pub namespace: String,
    pub auth_type: AuthTypeInput,
    /// Optional secret token — stored in CredentialStore, never logged.
    pub token: Option<String>,
}

/// Response for `POST /api/connectors/generic`.
#[derive(Serialize)]
pub struct CreateGenericSourceResponse {
    pub source_id: String,
}

/// A single entry in the `GET /api/connectors` response.
#[derive(Serialize)]
pub struct ConnectorInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub connector_type: String,
    pub enabled: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ---------------------------------------------------------------------------
// Business logic (called from HTTP handlers and unit tests)
// ---------------------------------------------------------------------------

/// Creates and starts a new generic source.
///
/// Generates a UUIDv4 source ID, persists the config in `GenericConfigStore`,
/// stores the token in `CredentialStore` under `user_id="generic"`, and
/// starts the Bento subprocess via `GenericRunner`.
pub async fn handle_create_generic_source(
    state: &ApiState,
    req: CreateGenericSourceRequest,
) -> Result<String> {
    let source_id = uuid::Uuid::new_v4().to_string();
    let auth_type = req.auth_type.into();
    let token = req.token;

    let config = GenericSourceConfig {
        id: source_id.clone(),
        name: req.name,
        url: req.url,
        poll_interval_secs: req.poll_interval_secs,
        entity_key: req.entity_key,
        namespace: req.namespace,
        auth_type,
        created_at: Utc::now(),
    };

    state.config_store.insert(&config)?;

    if let Some(ref t) = token {
        let creds = Credentials {
            access_token: t.clone(),
            refresh_token: None,
            expires_at: None,
        };
        state
            .credential_store
            .store("generic", &source_id, &creds)?;
    }

    state.runner.start_source(&config, token).await?;

    info!(source_id = %source_id, name = %config.name, "Generic source created");
    Ok(source_id)
}

/// Stops and removes a generic source.
///
/// Kills the Bento subprocess, deletes the config from SQLite, and removes
/// credentials from `CredentialStore` (best-effort — no error if not found).
pub async fn handle_delete_generic_source(state: &ApiState, source_id: &str) -> Result<()> {
    state.runner.stop_source(source_id).await?;
    state.config_store.delete(source_id)?;
    // Best-effort credential cleanup (may not exist if auth_type was None)
    let _ = state.credential_store.delete("generic", source_id);
    info!(source_id = %source_id, "Generic source deleted");
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn post_generic_source(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateGenericSourceRequest>,
) -> Result<(StatusCode, Json<CreateGenericSourceResponse>), AppError> {
    let source_id = handle_create_generic_source(&state, req)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateGenericSourceResponse { source_id }),
    ))
}

async fn delete_generic_source(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    handle_delete_generic_source(&state, &source_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_connectors(State(state): State<Arc<ApiState>>) -> Json<Vec<ConnectorInfo>> {
    let mut connectors: Vec<ConnectorInfo> = Vec::new();

    // Built-in connectors from registry
    for c in get_all_connectors() {
        connectors.push(ConnectorInfo {
            name: c.name().to_string(),
            connector_type: "builtin".to_string(),
            enabled: true,
            status: "running".to_string(),
            source_id: None,
            last_started: None,
            last_error: None,
        });
    }

    // Generic connectors from config store + runner status
    let generic_configs = state.config_store.list().unwrap_or_else(|e| {
        warn!(error = %e, "Failed to list generic source configs");
        vec![]
    });
    let statuses = state.runner.status();

    for config in generic_configs {
        let status_entry = statuses.iter().find(|s| s.source_id == config.id);
        let (status, last_started, last_error) = match status_entry {
            Some(s) => {
                let st = if s.last_error.is_some() { "error" } else { "running" };
                (
                    st.to_string(),
                    s.last_started.map(|dt| dt.to_rfc3339()),
                    s.last_error.clone(),
                )
            }
            None => ("stopped".to_string(), None, None),
        };

        connectors.push(ConnectorInfo {
            name: config.name,
            connector_type: "generic".to_string(),
            enabled: true,
            status,
            source_id: Some(config.id),
            last_started,
            last_error,
        });
    }

    Json(connectors)
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

enum AppError {
    Internal(String),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let AppError::Internal(msg) = self;
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: msg }),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn create_router(state: ApiState) -> Router {
    Router::new()
        .route("/api/connectors/generic", post(post_generic_source))
        .route(
            "/api/connectors/generic/:source_id",
            delete(delete_generic_source),
        )
        .route("/api/connectors", get(list_connectors))
        .with_state(Arc::new(state))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> ApiState {
        let config_store = Arc::new(GenericConfigStore::new(":memory:").unwrap());
        let credential_store = Arc::new(
            CredentialStore::new(":memory:", &base64::encode([0u8; 32])).unwrap(),
        );
        let runner = Arc::new(GenericRunner::new(
            Arc::clone(&config_store),
            "http://localhost:3000".to_string(),
        ));
        ApiState {
            config_store,
            runner,
            credential_store,
        }
    }

    fn make_request(name: &str) -> CreateGenericSourceRequest {
        CreateGenericSourceRequest {
            name: name.to_string(),
            url: "https://api.coingecko.com/api/v3/simple/price".to_string(),
            poll_interval_secs: 300,
            entity_key: "bitcoin".to_string(),
            namespace: "personal".to_string(),
            auth_type: AuthTypeInput::Plain("none".to_string()),
            token: None,
        }
    }

    #[tokio::test]
    async fn test_post_generic_source_stores_config() {
        let state = make_state();
        let source_id = handle_create_generic_source(&state, make_request("Bitcoin Price"))
            .await
            .unwrap();

        let stored = state.config_store.get(&source_id).unwrap();
        assert!(stored.is_some(), "config should be stored after POST");
        let config = stored.unwrap();
        assert_eq!(config.name, "Bitcoin Price");
        assert_eq!(config.url, "https://api.coingecko.com/api/v3/simple/price");
        assert_eq!(config.poll_interval_secs, 300);
        assert_eq!(config.entity_key, "bitcoin");
        assert_eq!(config.namespace, "personal");
    }

    #[tokio::test]
    async fn test_delete_generic_source_removes_config() {
        let state = make_state();
        // Create a source first
        let source_id = handle_create_generic_source(&state, make_request("Test Source"))
            .await
            .unwrap();
        assert!(
            state.config_store.get(&source_id).unwrap().is_some(),
            "config should exist before delete"
        );

        // Delete it
        handle_delete_generic_source(&state, &source_id)
            .await
            .unwrap();

        // Config should be gone
        let stored = state.config_store.get(&source_id).unwrap();
        assert!(stored.is_none(), "config should be removed after DELETE");
    }
}
