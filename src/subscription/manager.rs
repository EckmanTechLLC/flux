use crate::state::{EntityDeleted, MetricsUpdate, StateEngine, StateUpdate};
use crate::subscription::protocol::{
    ClientMessage, EntityDeletedMessage, MetricsUpdateMessage, StateUpdateMessage,
};
use axum::extract::ws::{Message, WebSocket};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Manages a single WebSocket connection with entity subscriptions
pub struct ConnectionManager {
    /// Set of entity IDs this connection is subscribed to
    subscriptions: HashSet<String>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: HashSet::new(),
        }
    }

    /// Handle WebSocket connection lifecycle
    pub async fn handle(
        mut self,
        mut socket: WebSocket,
        mut state_rx: broadcast::Receiver<StateUpdate>,
        mut metrics_rx: broadcast::Receiver<MetricsUpdate>,
        mut deletion_rx: broadcast::Receiver<EntityDeleted>,
        state_engine: Arc<StateEngine>,
    ) {
        // Increment WebSocket connection count
        state_engine.metrics.increment_ws_connection();
        info!("WebSocket connection established");

        loop {
            tokio::select! {
                // Handle incoming client messages
                Some(msg) = socket.recv() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.handle_client_message(&mut socket, &text).await {
                                error!(error = %e, "Error handling client message");
                            }
                        }
                        Ok(Message::Close(_)) => {
                            info!("WebSocket client disconnected");
                            break;
                        }
                        Ok(Message::Ping(data)) => {
                            if let Err(e) = socket.send(Message::Pong(data)).await {
                                error!(error = %e, "Failed to send pong");
                                break;
                            }
                        }
                        Ok(_) => {
                            // Ignore binary, pong messages
                        }
                        Err(e) => {
                            warn!(error = %e, "WebSocket error");
                            break;
                        }
                    }
                }

                // Handle state updates from broadcast channel
                result = state_rx.recv() => {
                    match result {
                        Ok(update) => {
                            if self.should_forward_update(&update) {
                                if let Err(e) = self.send_state_update(&mut socket, update).await {
                                    error!(error = %e, "Failed to send state update");
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped = skipped, "WebSocket lagged, skipped state updates");
                            // Continue processing
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("State broadcast channel closed");
                            break;
                        }
                    }
                }

                // Handle metrics updates from broadcast channel
                result = metrics_rx.recv() => {
                    match result {
                        Ok(metrics) => {
                            if let Err(e) = self.send_metrics_update(&mut socket, metrics).await {
                                error!(error = %e, "Failed to send metrics update");
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped = skipped, "WebSocket lagged, skipped metrics updates");
                            // Continue processing
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("Metrics broadcast channel closed");
                            break;
                        }
                    }
                }

                // Handle entity deletion events from broadcast channel
                result = deletion_rx.recv() => {
                    match result {
                        Ok(deleted) => {
                            if let Err(e) = self.send_entity_deleted(&mut socket, deleted).await {
                                error!(error = %e, "Failed to send entity deleted");
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped = skipped, "WebSocket lagged, skipped deletion events");
                            // Continue processing
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("Deletion broadcast channel closed");
                            break;
                        }
                    }
                }

                else => {
                    break;
                }
            }
        }

        // Decrement WebSocket connection count
        state_engine.metrics.decrement_ws_connection();
        info!("WebSocket connection closed");
    }

    /// Handle client message (subscribe/unsubscribe)
    async fn handle_client_message(
        &mut self,
        _socket: &mut WebSocket,
        text: &str,
    ) -> anyhow::Result<()> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::Subscribe { entity_id } => {
                info!(entity_id = %entity_id, "Client subscribed to entity");
                self.subscriptions.insert(entity_id);
            }
            ClientMessage::Unsubscribe { entity_id } => {
                info!(entity_id = %entity_id, "Client unsubscribed from entity");
                self.subscriptions.remove(&entity_id);
            }
        }

        Ok(())
    }

    /// Check if update should be forwarded to this connection
    fn should_forward_update(&self, update: &StateUpdate) -> bool {
        // If no subscriptions, forward all updates
        if self.subscriptions.is_empty() {
            return true;
        }

        // Check for wildcard subscription
        if self.subscriptions.contains("*") {
            return true;
        }

        // Otherwise, only forward if subscribed to this entity
        self.subscriptions.contains(&update.entity_id)
    }

    /// Send state update to client
    async fn send_state_update(
        &self,
        socket: &mut WebSocket,
        update: StateUpdate,
    ) -> anyhow::Result<()> {
        let msg = StateUpdateMessage::from(update);
        let json = serde_json::to_string(&msg)?;
        socket.send(Message::Text(json)).await?;
        Ok(())
    }

    /// Send metrics update to client
    async fn send_metrics_update(
        &self,
        socket: &mut WebSocket,
        metrics: MetricsUpdate,
    ) -> anyhow::Result<()> {
        let msg = MetricsUpdateMessage::from(metrics);
        let json = serde_json::to_string(&msg)?;
        socket.send(Message::Text(json)).await?;
        Ok(())
    }

    /// Send entity deleted to client
    async fn send_entity_deleted(
        &self,
        socket: &mut WebSocket,
        deleted: EntityDeleted,
    ) -> anyhow::Result<()> {
        let msg = EntityDeletedMessage::from(deleted);
        let json = serde_json::to_string(&msg)?;
        socket.send(Message::Text(json)).await?;
        Ok(())
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
