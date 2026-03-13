//! Connector Manager HTTP API.
//!
//! Routes:
//! - `POST /api/connectors/generic` — create a new generic (Bento) source
//! - `DELETE /api/connectors/generic/:source_id` — remove a generic source
//! - `POST /api/connectors/named` — create a new named (Singer) source
//! - `DELETE /api/connectors/named/:source_id` — remove a named source
//! - `POST /api/connectors/named/:source_id/sync` — trigger one-shot sync
//! - `POST /api/connectors/i3x` — create a new i3X source
//! - `DELETE /api/connectors/i3x/:source_id` — remove an i3X source
//! - `POST /api/connectors/i3x/:source_id/sync` — trigger one-shot i3X sync
//! - `GET /api/connectors` — list all connectors (builtin + generic + named + i3x)
//! - `GET /api/connectors/taps` — return the Meltano Hub tap catalog

use crate::generic_config::{AuthType, GenericConfigStore, GenericSourceConfig};
use crate::i3x_config::{I3xConfigStore, I3xSourceConfig};
use crate::named_config::NamedSourceConfig;
use crate::registry::get_all_connectors;
use crate::runners::generic::GenericRunner;
use crate::runners::i3x::I3xRunner;
use crate::runners::named::{NamedRunner, TapCatalogEntry, TapCatalogStore};
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
    pub tap_catalog: Arc<TapCatalogStore>,
    pub named_runner: Arc<NamedRunner>,
    pub i3x_config_store: Arc<I3xConfigStore>,
    pub i3x_runner: Arc<I3xRunner>,
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
    /// Optional Flux namespace token for auth-enabled Flux instances.
    pub flux_namespace_token: Option<String>,
}

/// Response for `POST /api/connectors/generic`.
#[derive(Serialize)]
pub struct CreateGenericSourceResponse {
    pub source_id: String,
}

/// Request body for `POST /api/connectors/named`.
#[derive(Deserialize)]
pub struct CreateNamedSourceRequest {
    pub tap_name: String,
    pub namespace: String,
    pub entity_key_field: String,
    /// Tap configuration JSON (credentials + settings).
    pub config_json: String,
    pub poll_interval_secs: u64,
    /// Optional Flux namespace token for auth-enabled Flux instances.
    pub flux_namespace_token: Option<String>,
}

/// Response for `POST /api/connectors/named`.
#[derive(Serialize)]
pub struct CreateNamedSourceResponse {
    pub source_id: String,
}

/// Request body for `POST /api/connectors/i3x`.
#[derive(Deserialize)]
pub struct CreateI3xSourceRequest {
    pub name: String,
    pub base_url: String,
    pub namespace: String,
    pub api_key: String,
    pub flux_namespace_token: String,
}

/// Response for `POST /api/connectors/i3x`.
#[derive(Serialize)]
pub struct CreateI3xSourceResponse {
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
        flux_namespace_token: req.flux_namespace_token,
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

/// Creates and starts a new named Singer tap source.
///
/// Generates a UUIDv4 source ID, persists the config in `NamedConfigStore`,
/// and starts the Singer subprocess via `NamedRunner`.
pub async fn handle_create_named_source(
    state: &ApiState,
    req: CreateNamedSourceRequest,
) -> Result<String> {
    let source_id = uuid::Uuid::new_v4().to_string();
    let config = NamedSourceConfig {
        id: source_id.clone(),
        tap_name: req.tap_name,
        namespace: req.namespace,
        entity_key_field: req.entity_key_field,
        config_json: req.config_json,
        poll_interval_secs: req.poll_interval_secs,
        created_at: Utc::now(),
        flux_namespace_token: req.flux_namespace_token,
    };
    state.named_runner.store.insert(&config)?;
    state.named_runner.start_source(&config).await?;
    info!(source_id = %source_id, tap = %config.tap_name, "Named source created");
    Ok(source_id)
}

/// Triggers an immediate one-shot sync for a named Singer tap source.
///
/// Fire-and-forget: returns `Ok(())` as soon as the background task is spawned.
/// Returns `Err` if the source is not found.
pub async fn handle_sync_named_source(state: &ApiState, source_id: &str) -> Result<()> {
    state.named_runner.trigger_sync(source_id).await
}

/// Stops and removes a named Singer tap source.
///
/// Aborts the background task, deletes the config from SQLite, and removes
/// any temp files for the source.
pub async fn handle_delete_named_source(state: &ApiState, source_id: &str) -> Result<()> {
    state.named_runner.stop_source(source_id).await?;
    state.named_runner.store.delete(source_id)?;
    info!(source_id = %source_id, "Named source deleted");
    Ok(())
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

/// Creates and starts a new i3X source.
///
/// Generates a UUIDv4 source ID, persists config in `I3xConfigStore`,
/// stores the API key in `CredentialStore` under `user_id="i3x"`, and
/// starts the SSE streaming task via `I3xRunner`.
pub async fn handle_create_i3x_source(
    state: &ApiState,
    req: CreateI3xSourceRequest,
) -> Result<String> {
    let source_id = uuid::Uuid::new_v4().to_string();
    let config = I3xSourceConfig {
        id: source_id.clone(),
        name: req.name,
        base_url: req.base_url,
        namespace: req.namespace,
        flux_namespace_token: req.flux_namespace_token,
        created_at: Utc::now(),
    };
    state.i3x_config_store.insert(&config)?;

    let creds = Credentials {
        access_token: req.api_key.clone(),
        refresh_token: None,
        expires_at: None,
    };
    state.credential_store.store("i3x", &source_id, &creds)?;

    state.i3x_runner.start_source(&config, req.api_key).await?;
    info!(source_id = %source_id, name = %config.name, "i3X source created");
    Ok(source_id)
}

/// Stops and removes an i3X source.
///
/// Aborts the background SSE task, deletes config from SQLite, and removes
/// credentials from `CredentialStore` (best-effort).
pub async fn handle_delete_i3x_source(state: &ApiState, source_id: &str) -> Result<()> {
    state.i3x_runner.stop_source(source_id).await?;
    state.i3x_config_store.delete(source_id)?;
    let _ = state.credential_store.delete("i3x", source_id);
    info!(source_id = %source_id, "i3X source deleted");
    Ok(())
}

/// Triggers a one-shot sync for an i3X source.
///
/// Returns `Err` if the source is not found; otherwise fire-and-forget.
pub async fn handle_sync_i3x_source(state: &ApiState, source_id: &str) -> Result<()> {
    let config = state
        .i3x_config_store
        .get(source_id)?
        .ok_or_else(|| anyhow::anyhow!("i3X source {} not found", source_id))?;
    let api_key = state
        .credential_store
        .get("i3x", source_id)?
        .map(|c| c.access_token)
        .unwrap_or_default();
    state.i3x_runner.trigger_sync(&config, api_key).await
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn post_named_source(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateNamedSourceRequest>,
) -> Result<(StatusCode, Json<CreateNamedSourceResponse>), AppError> {
    let source_id = handle_create_named_source(&state, req)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateNamedSourceResponse { source_id }),
    ))
}

async fn delete_named_source(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    handle_delete_named_source(&state, &source_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_sync_named_source(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    handle_sync_named_source(&state, &source_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::ACCEPTED)
}

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

async fn post_i3x_source(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<CreateI3xSourceRequest>,
) -> Result<(StatusCode, Json<CreateI3xSourceResponse>), AppError> {
    let source_id = handle_create_i3x_source(&state, req)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateI3xSourceResponse { source_id }),
    ))
}

async fn delete_i3x_source(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    handle_delete_i3x_source(&state, &source_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_sync_i3x_source(
    State(state): State<Arc<ApiState>>,
    Path(source_id): Path<String>,
) -> Result<StatusCode, AppError> {
    handle_sync_i3x_source(&state, &source_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::ACCEPTED)
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

    // Named connectors from config store + runner status
    let named_configs = state.named_runner.store.list().unwrap_or_else(|e| {
        warn!(error = %e, "Failed to list named source configs");
        vec![]
    });
    let named_statuses = state.named_runner.status();

    for config in named_configs {
        let status_entry = named_statuses.iter().find(|s| s.source_id == config.id);
        let (status, last_started, last_error) = match status_entry {
            Some(s) => {
                let st = if s.last_error.is_some() { "error" } else { "running" };
                (
                    st.to_string(),
                    s.last_run.map(|dt| dt.to_rfc3339()),
                    s.last_error.clone(),
                )
            }
            None => ("stopped".to_string(), None, None),
        };

        connectors.push(ConnectorInfo {
            name: config.tap_name,
            connector_type: "named".to_string(),
            enabled: true,
            status,
            source_id: Some(config.id),
            last_started,
            last_error,
        });
    }

    // i3X connectors from config store + runner status
    let i3x_configs = state.i3x_config_store.list().unwrap_or_else(|e| {
        warn!(error = %e, "Failed to list i3X source configs");
        vec![]
    });
    let i3x_statuses = state.i3x_runner.status();

    for config in i3x_configs {
        let status_entry = i3x_statuses.iter().find(|s| s.source_id == config.id);
        let (status, last_started, last_error) = match status_entry {
            Some(s) => {
                let st = if s.last_error.is_some() { "error" } else { "running" };
                (
                    st.to_string(),
                    s.last_event.map(|dt| dt.to_rfc3339()),
                    s.last_error.clone(),
                )
            }
            None => ("stopped".to_string(), None, None),
        };
        connectors.push(ConnectorInfo {
            name: config.name,
            connector_type: "i3x".to_string(),
            enabled: true,
            status,
            source_id: Some(config.id),
            last_started,
            last_error,
        });
    }

    Json(connectors)
}

async fn get_tap_catalog(State(state): State<Arc<ApiState>>) -> Json<Vec<TapCatalogEntry>> {
    Json(state.tap_catalog.list())
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
        .route("/api/connectors/named", post(post_named_source))
        .route(
            "/api/connectors/named/:source_id",
            delete(delete_named_source),
        )
        .route(
            "/api/connectors/named/:source_id/sync",
            post(post_sync_named_source),
        )
        .route("/api/connectors/generic", post(post_generic_source))
        .route(
            "/api/connectors/generic/:source_id",
            delete(delete_generic_source),
        )
        .route("/api/connectors/i3x", post(post_i3x_source))
        .route(
            "/api/connectors/i3x/:source_id",
            delete(delete_i3x_source),
        )
        .route(
            "/api/connectors/i3x/:source_id/sync",
            post(post_sync_i3x_source),
        )
        .route("/api/connectors", get(list_connectors))
        .route("/api/connectors/taps", get(get_tap_catalog))
        .with_state(Arc::new(state))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i3x_config::I3xConfigStore;
    use crate::named_config::NamedConfigStore;

    fn make_state() -> ApiState {
        let config_store = Arc::new(GenericConfigStore::new(":memory:").unwrap());
        let named_store = Arc::new(NamedConfigStore::new(":memory:").unwrap());
        let i3x_config_store = Arc::new(I3xConfigStore::new(":memory:").unwrap());
        let credential_store = Arc::new(
            CredentialStore::new(":memory:", &base64::encode([0u8; 32])).unwrap(),
        );
        let runner = Arc::new(GenericRunner::new(
            Arc::clone(&config_store),
            "http://localhost:3000".to_string(),
        ));
        let named_runner = Arc::new(NamedRunner::new(
            Arc::clone(&named_store),
            "http://localhost:3000".to_string(),
        ));
        let i3x_runner = Arc::new(I3xRunner::new("http://localhost:3000".to_string()));
        let tap_catalog = Arc::new(TapCatalogStore::new("/nonexistent/test-catalog.json"));
        ApiState {
            config_store,
            runner,
            credential_store,
            tap_catalog,
            named_runner,
            i3x_config_store,
            i3x_runner,
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
            flux_namespace_token: None,
        }
    }

    fn make_named_request(tap: &str) -> CreateNamedSourceRequest {
        CreateNamedSourceRequest {
            tap_name: tap.to_string(),
            namespace: "personal".to_string(),
            entity_key_field: "id".to_string(),
            config_json: r#"{"access_token": "ghp_test"}"#.to_string(),
            poll_interval_secs: 3600,
            flux_namespace_token: None,
        }
    }

    #[tokio::test]
    async fn test_post_named_source_stores_config() {
        let state = make_state();
        let source_id = handle_create_named_source(&state, make_named_request("tap-github"))
            .await
            .unwrap();

        let stored = state.named_runner.store.get(&source_id).unwrap();
        assert!(stored.is_some(), "config should be stored after POST");
        let config = stored.unwrap();
        assert_eq!(config.tap_name, "tap-github");
        assert_eq!(config.namespace, "personal");
        assert_eq!(config.entity_key_field, "id");
        assert_eq!(config.poll_interval_secs, 3600);
    }

    #[tokio::test]
    async fn test_delete_named_source_removes_config() {
        let state = make_state();
        let source_id = handle_create_named_source(&state, make_named_request("tap-github"))
            .await
            .unwrap();
        assert!(
            state.named_runner.store.get(&source_id).unwrap().is_some(),
            "config should exist before delete"
        );

        handle_delete_named_source(&state, &source_id).await.unwrap();

        let stored = state.named_runner.store.get(&source_id).unwrap();
        assert!(stored.is_none(), "config should be removed after DELETE");
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

    fn make_i3x_request(name: &str) -> CreateI3xSourceRequest {
        CreateI3xSourceRequest {
            name: name.to_string(),
            base_url: "https://demo.i3x.dev".to_string(),
            namespace: "flux-manufacturing".to_string(),
            api_key: "test-api-key".to_string(),
            flux_namespace_token: "tok-abc123".to_string(),
        }
    }

    #[tokio::test]
    async fn test_post_i3x_source_stores_config() {
        let state = make_state();
        let source_id = handle_create_i3x_source(&state, make_i3x_request("Factory A"))
            .await
            .unwrap();

        let stored = state.i3x_config_store.get(&source_id).unwrap();
        assert!(stored.is_some(), "config should be stored after POST");
        let config = stored.unwrap();
        assert_eq!(config.name, "Factory A");
        assert_eq!(config.base_url, "https://demo.i3x.dev");
        assert_eq!(config.namespace, "flux-manufacturing");
        assert_eq!(config.flux_namespace_token, "tok-abc123");
    }

    #[tokio::test]
    async fn test_post_i3x_source_stores_credentials() {
        let state = make_state();
        let source_id = handle_create_i3x_source(&state, make_i3x_request("Factory B"))
            .await
            .unwrap();

        let creds = state.credential_store.get("i3x", &source_id).unwrap();
        assert!(creds.is_some(), "credentials should be stored");
        assert_eq!(creds.unwrap().access_token, "test-api-key");
    }

    #[tokio::test]
    async fn test_delete_i3x_source_removes_config() {
        let state = make_state();
        let source_id = handle_create_i3x_source(&state, make_i3x_request("Factory C"))
            .await
            .unwrap();
        assert!(
            state.i3x_config_store.get(&source_id).unwrap().is_some(),
            "config should exist before delete"
        );

        handle_delete_i3x_source(&state, &source_id).await.unwrap();

        assert!(
            state.i3x_config_store.get(&source_id).unwrap().is_none(),
            "config should be removed after DELETE"
        );
    }

    #[tokio::test]
    async fn test_sync_i3x_source_not_found_returns_error() {
        let state = make_state();
        let result = handle_sync_i3x_source(&state, "nonexistent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_i3x_source_appears_in_list() {
        let state = make_state();
        handle_create_i3x_source(&state, make_i3x_request("Test i3X"))
            .await
            .unwrap();

        let configs = state.i3x_config_store.list().unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "Test i3X");
        assert_eq!(configs[0].namespace, "flux-manufacturing");
    }
}
