use crate::state::{EntityDeleted, MetricsUpdate, StateEngine, StateUpdate};
use crate::subscription::protocol::{
    ClientMessage, EntityDeletedMessage, MetricsUpdateMessage, QosTier,
    StateDeltaMessage, StateUpdateMessage, Subscription,
};
use axum::extract::ws::{Message, WebSocket};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Maximum buffered messages for reliable QoS subscriptions
const RELIABLE_BUFFER_CAP: usize = 500;

/// Manages a single WebSocket connection with entity subscriptions.
///
/// Extended with SingularisPrime-inspired messaging:
/// - Prefix/glob subscriptions (SP: `event.subscribe(prefix, filter)`)
/// - Property-level filtering (SP: domain grants scoping)
/// - QoS tiers: realtime (drop on lag), reliable (buffer), snapshot (poll)
/// - Delta compression for numeric property updates
pub struct ConnectionManager {
    /// Active subscriptions for this connection
    subscriptions: Vec<Subscription>,
    /// Buffer for reliable QoS — holds messages during client lag
    reliable_buffer: VecDeque<String>,
    /// Whether the client is currently lagging (for reliable QoS)
    client_lagging: bool,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: Vec::new(),
            reliable_buffer: VecDeque::new(),
            client_lagging: false,
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
                            if let Err(e) = self.handle_client_message(&mut socket, &text, &state_engine).await {
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
                        Ok(_) => {}
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
                            if let Some(sub) = self.find_matching_subscription(&update) {
                                let json = self.format_update(&update, &sub);
                                if let Some(json) = json {
                                    if let Err(e) = self.send_with_qos(&mut socket, json, sub.qos).await {
                                        error!(error = %e, "Failed to send state update");
                                        break;
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped = skipped, "WebSocket lagged, skipped state updates");
                            self.client_lagging = true;
                            // Flush reliable buffer if we have one
                            if let Err(e) = self.flush_reliable_buffer(&mut socket).await {
                                error!(error = %e, "Failed to flush reliable buffer");
                                break;
                            }
                            self.client_lagging = false;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("State broadcast channel closed");
                            break;
                        }
                    }
                }

                // Handle metrics updates
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
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            error!("Metrics broadcast channel closed");
                            break;
                        }
                    }
                }

                // Handle entity deletion events
                result = deletion_rx.recv() => {
                    match result {
                        Ok(deleted) => {
                            // Forward deletion if any subscription matches
                            let should_forward = self.subscriptions.is_empty()
                                || self.subscriptions.iter().any(|s| s.matches_entity(&deleted.entity_id));
                            if should_forward {
                                if let Err(e) = self.send_entity_deleted(&mut socket, deleted).await {
                                    error!(error = %e, "Failed to send entity deleted");
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped = skipped, "WebSocket lagged, skipped deletion events");
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
        socket: &mut WebSocket,
        text: &str,
        state_engine: &Arc<StateEngine>,
    ) -> anyhow::Result<()> {
        let msg: ClientMessage = serde_json::from_str(text)?;

        match msg {
            ClientMessage::Subscribe {
                entity_id,
                properties,
                qos,
                delta,
            } => {
                info!(
                    entity_id = %entity_id,
                    properties = ?properties,
                    qos = ?qos,
                    delta = delta,
                    "Client subscribed"
                );

                let sub = Subscription::from_subscribe(&entity_id, properties, qos, delta);

                // For snapshot QoS, send current state immediately then don't stream
                if qos == QosTier::Snapshot {
                    self.send_snapshot(socket, &sub, state_engine).await?;
                }

                self.subscriptions.push(sub);
            }
            ClientMessage::Unsubscribe { entity_id } => {
                info!(entity_id = %entity_id, "Client unsubscribed");
                self.subscriptions.retain(|s| s.pattern != entity_id);
            }
        }

        Ok(())
    }

    /// Find the first subscription that matches this update
    fn find_matching_subscription(&self, update: &StateUpdate) -> Option<Subscription> {
        // If no subscriptions, forward all (backward compat)
        if self.subscriptions.is_empty() {
            return Some(Subscription::from_subscribe("*", None, QosTier::Realtime, false));
        }

        self.subscriptions
            .iter()
            .find(|s| s.matches_update(update))
            .cloned()
    }

    /// Format a state update according to subscription settings (absolute vs delta)
    fn format_update(&self, update: &StateUpdate, sub: &Subscription) -> Option<String> {
        if sub.delta {
            // Try delta first; fall back to absolute for non-numeric
            if let Some(delta_msg) = StateDeltaMessage::try_from_update(update) {
                return serde_json::to_string(&delta_msg).ok();
            }
        }
        let msg = StateUpdateMessage::from(update.clone());
        serde_json::to_string(&msg).ok()
    }

    /// Send a message respecting QoS tier
    async fn send_with_qos(
        &mut self,
        socket: &mut WebSocket,
        json: String,
        qos: QosTier,
    ) -> anyhow::Result<()> {
        match qos {
            QosTier::Realtime => {
                // Fire and forget — if send fails, we drop it
                socket.send(Message::Text(json)).await?;
            }
            QosTier::Reliable => {
                if self.client_lagging {
                    // Buffer during lag
                    self.reliable_buffer.push_back(json);
                    if self.reliable_buffer.len() > RELIABLE_BUFFER_CAP {
                        self.reliable_buffer.pop_front(); // Drop oldest
                        debug!("Reliable buffer overflow, dropped oldest message");
                    }
                } else {
                    socket.send(Message::Text(json)).await?;
                }
            }
            QosTier::Snapshot => {
                // Snapshot mode doesn't stream — only sends on subscribe
            }
        }
        Ok(())
    }

    /// Flush buffered reliable messages to the client
    async fn flush_reliable_buffer(
        &mut self,
        socket: &mut WebSocket,
    ) -> anyhow::Result<()> {
        let count = self.reliable_buffer.len();
        if count == 0 {
            return Ok(());
        }

        info!(count = count, "Flushing reliable QoS buffer");

        while let Some(json) = self.reliable_buffer.pop_front() {
            socket.send(Message::Text(json)).await?;
        }

        Ok(())
    }

    /// Send current entity state snapshot for matching entities (snapshot QoS)
    async fn send_snapshot(
        &self,
        socket: &mut WebSocket,
        sub: &Subscription,
        state_engine: &Arc<StateEngine>,
    ) -> anyhow::Result<()> {
        let entities = state_engine.get_all_entities();
        let mut sent = 0u32;

        for entity in entities {
            if sub.matches_entity(&entity.id) {
                for (property, value) in &entity.properties {
                    if sub.matches_property(property) {
                        let msg = StateUpdateMessage {
                            msg_type: "state_update".to_string(),
                            entity_id: entity.id.clone(),
                            property: property.clone(),
                            value: value.clone(),
                            timestamp: entity.last_updated,
                        };
                        let json = serde_json::to_string(&msg)?;
                        socket.send(Message::Text(json)).await?;
                        sent += 1;
                    }
                }
            }
        }

        info!(count = sent, pattern = %sub.pattern, "Snapshot delivered");
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
