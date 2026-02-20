use crate::config::SharedRuntimeConfig;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// State for the admin API.
#[derive(Clone)]
pub struct AdminAppState {
    pub runtime_config: SharedRuntimeConfig,
    /// Required bearer token for PUT /api/admin/config. None = PUT disabled.
    pub admin_token: Option<String>,
}

/// Partial update body — only fields present in the request are changed.
#[derive(Deserialize)]
pub struct RuntimeConfigUpdate {
    pub rate_limit_enabled: Option<bool>,
    pub rate_limit_per_namespace_per_minute: Option<u64>,
    pub body_size_limit_single_bytes: Option<usize>,
    pub body_size_limit_batch_bytes: Option<usize>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

pub fn create_admin_router(state: AdminAppState) -> Router {
    Router::new()
        .route(
            "/api/admin/config",
            get(get_config).put(put_config),
        )
        .with_state(Arc::new(state))
}

/// GET /api/admin/config — returns current RuntimeConfig.
async fn get_config(
    State(state): State<Arc<AdminAppState>>,
) -> Response {
    let cfg = state
        .runtime_config
        .read()
        .expect("RuntimeConfig lock poisoned")
        .clone();
    Json(cfg).into_response()
}

/// PUT /api/admin/config — partial update. Requires FLUX_ADMIN_TOKEN bearer.
async fn put_config(
    State(state): State<Arc<AdminAppState>>,
    headers: HeaderMap,
    Json(update): Json<RuntimeConfigUpdate>,
) -> Response {
    // Admin token check
    if !validate_admin_token(&headers, &state.admin_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
            .into_response();
    }

    // Apply partial update
    let mut cfg = state
        .runtime_config
        .write()
        .expect("RuntimeConfig lock poisoned");

    if let Some(v) = update.rate_limit_enabled {
        cfg.rate_limit_enabled = v;
    }
    if let Some(v) = update.rate_limit_per_namespace_per_minute {
        cfg.rate_limit_per_namespace_per_minute = v;
    }
    if let Some(v) = update.body_size_limit_single_bytes {
        cfg.body_size_limit_single_bytes = v;
    }
    if let Some(v) = update.body_size_limit_batch_bytes {
        cfg.body_size_limit_batch_bytes = v;
    }

    Json(cfg.clone()).into_response()
}

/// Returns true if the bearer token in `Authorization` matches the expected admin token.
/// Returns true (no restriction) when `expected` is None.
fn validate_admin_token(headers: &HeaderMap, expected: &Option<String>) -> bool {
    let Some(expected_token) = expected else {
        // No admin token configured → PUT is unrestricted (dev mode)
        return true;
    };

    let Some(auth_header) = headers.get("Authorization") else {
        return false;
    };
    let Ok(value) = auth_header.to_str() else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };

    token == expected_token
}
