use crate::state::StateEngine;
use crate::subscription::ConnectionManager;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use std::sync::Arc;
use tracing::info;

/// Shared application state for WebSocket handler
#[derive(Clone)]
pub struct WsAppState {
    pub state_engine: Arc<StateEngine>,
}

/// GET /api/ws - WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WsAppState>>,
) -> Response {
    info!("WebSocket upgrade request received");
    ws.on_upgrade(|socket| handle_socket(socket, state))
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
