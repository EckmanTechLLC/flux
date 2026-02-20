use crate::namespace::NamespaceRegistry;
use crate::state::StateEngine;
use crate::subscription::ConnectionManager;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        Query, Request, State,
    },
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

/// Query parameters for WebSocket upgrade
#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

/// Shared application state for WebSocket handler
#[derive(Clone)]
pub struct WsAppState {
    pub state_engine: Arc<StateEngine>,
    pub namespace_registry: Arc<NamespaceRegistry>,
    pub auth_enabled: bool,
}

/// Auth middleware: validates ?token= when auth_enabled=true.
///
/// Runs as a tower layer BEFORE WebSocket upgrade extraction so 401 can be
/// returned cleanly without requiring a valid upgrade request in tests.
async fn ws_auth(
    State(state): State<Arc<WsAppState>>,
    Query(params): Query<WsQuery>,
    req: Request,
    next: Next,
) -> Response {
    if state.auth_enabled {
        match params.token {
            Some(ref token) => {
                if state.namespace_registry.lookup_by_token(token).is_none() {
                    return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
                }
            }
            None => return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
        }
    }
    next.run(req).await
}

/// GET /api/ws - WebSocket upgrade handler (auth handled by ws_auth middleware)
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsAppState>>,
) -> Response {
    info!("WebSocket upgrade request received");
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Create WebSocket router with auth middleware applied
pub fn create_ws_router(state: Arc<WsAppState>) -> Router {
    Router::new()
        .route("/api/ws", get(ws_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), ws_auth))
        .with_state(state)
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<WsAppState>) {
    // Subscribe to state updates
    let state_rx = state.state_engine.subscribe();

    // Subscribe to metrics updates
    let metrics_rx = state.state_engine.subscribe_metrics();

    // Subscribe to deletion events
    let deletion_rx = state.state_engine.subscribe_deletions();

    // Create connection manager
    let manager = ConnectionManager::new();

    // Handle connection lifecycle
    manager
        .handle(
            socket,
            state_rx,
            metrics_rx,
            deletion_rx,
            Arc::clone(&state.state_engine),
        )
        .await;
}
