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
                            // Deletions were previously sent to every connection
                            // regardless of subscription, unlike updates. A
                            // consumer scoped to one namespace should not be told
                            // about deletions in another.
                            if self.matches(&deleted.entity_id) {
                                if let Err(e) = self.send_entity_deleted(&mut socket, deleted).await {
                                    error!(error = %e, "Failed to send entity deleted");
                                    break;
                                }
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

    /// Check whether this connection wants events for `entity_id`.
    ///
    /// Matching, in order:
    /// - no subscriptions at all → everything (unchanged default)
    /// - `*` → everything (unchanged)
    /// - an exact entity id (unchanged)
    /// - a trailing-`*` pattern, e.g. `flux-crypto/*` or `flux-*`
    ///
    /// The trailing-wildcard form is what lets a consumer take one namespace
    /// instead of the whole world. Without it, observer-gene had to subscribe to
    /// `*` and discard ~59k entities' updates client-side.
    fn matches(&self, entity_id: &str) -> bool {
        if self.subscriptions.is_empty() || self.subscriptions.contains("*") {
            return true;
        }
        if self.subscriptions.contains(entity_id) {
            return true;
        }
        self.subscriptions.iter().any(|pattern| {
            pattern
                .strip_suffix('*')
                .is_some_and(|prefix| entity_id.starts_with(prefix))
        })
    }

    /// Check if update should be forwarded to this connection
    fn should_forward_update(&self, update: &StateUpdate) -> bool {
        self.matches(&update.entity_id)
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


#[cfg(test)]
mod tests {
    use super::*;

    fn mgr(patterns: &[&str]) -> ConnectionManager {
        let mut m = ConnectionManager::new();
        for p in patterns {
            m.subscriptions.insert((*p).to_string());
        }
        m
    }

    #[test]
    fn no_subscriptions_matches_everything() {
        assert!(mgr(&[]).matches("flux-crypto/bitcoin"));
    }

    #[test]
    fn wildcard_matches_everything() {
        assert!(mgr(&["*"]).matches("anything/at-all"));
    }

    #[test]
    fn exact_id_still_works() {
        let m = mgr(&["flux-crypto/bitcoin"]);
        assert!(m.matches("flux-crypto/bitcoin"));
        assert!(!m.matches("flux-crypto/ethereum"));
    }

    #[test]
    fn namespace_prefix_scopes_to_one_namespace() {
        let m = mgr(&["flux-crypto/*"]);
        assert!(m.matches("flux-crypto/bitcoin"));
        assert!(m.matches("flux-crypto/ethereum"));
        assert!(!m.matches("flux-weather/london"));
    }

    #[test]
    fn broad_prefix_matches_across_namespaces() {
        // observer-gene's actual filter: every flux-* namespace, nothing else.
        let m = mgr(&["flux-*"]);
        assert!(m.matches("flux-ships-thames/MMSI-1"));
        assert!(m.matches("flux-weather/london"));
        assert!(!m.matches("knowledge-gene/steer"));
    }

    #[test]
    fn prefix_and_exact_combine() {
        // observer-gene wants all flux-* plus one specific foreign entity.
        let m = mgr(&["flux-*", "knowledge-gene/steer"]);
        assert!(m.matches("flux-crypto/bitcoin"));
        assert!(m.matches("knowledge-gene/steer"));
        assert!(!m.matches("knowledge-gene/state"));
    }

    #[test]
    fn bare_star_inside_a_pattern_is_not_a_prefix_match() {
        // Only a TRAILING star is a wildcard; this must not match everything.
        let m = mgr(&["flux-crypto/*"]);
        assert!(!m.matches("other/flux-crypto/x"));
    }
}
