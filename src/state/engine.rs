use crate::event::FluxEvent;
use crate::state::entity::{Entity, EntityDeleted, StateUpdate};
use crate::state::metrics::MetricsTracker;
use anyhow::{Context, Result};
use async_nats::jetstream;
use chrono::Utc;
use dashmap::DashMap;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// State engine maintains in-memory world state
pub struct StateEngine {
    /// Lock-free concurrent map for fast reads
    pub(crate) entities: Arc<DashMap<String, Entity>>,

    /// Broadcast channel for state change events
    state_tx: broadcast::Sender<StateUpdate>,

    /// Broadcast channel for entity deletion events
    deletion_tx: broadcast::Sender<EntityDeleted>,

    /// Last processed NATS sequence number
    last_processed_sequence: AtomicU64,

    /// True during NATS replay on startup; broadcasts are suppressed
    replaying: AtomicBool,

    /// Metrics tracker for monitoring
    pub metrics: MetricsTracker,

    /// Broadcast channel for metrics updates
    pub(crate) metrics_tx: broadcast::Sender<crate::state::metrics_broadcaster::MetricsUpdate>,
}

impl StateEngine {
    /// Create new state engine with broadcast channel
    pub fn new() -> Self {
        let (state_tx, _) = broadcast::channel(1000);
        let (deletion_tx, _) = broadcast::channel(100);
        let (metrics_tx, _) = broadcast::channel(10);

        Self {
            entities: Arc::new(DashMap::new()),
            state_tx,
            deletion_tx,
            last_processed_sequence: AtomicU64::new(0),
            replaying: AtomicBool::new(true),
            metrics: MetricsTracker::new(),
            metrics_tx,
        }
    }

    /// Update entity property (core state mutation)
    pub fn update_property(
        &self,
        entity_id: &str,
        property: &str,
        value: Value,
    ) -> StateUpdate {
        let now = Utc::now();

        // Get or create entity
        let mut entity = self
            .entities
            .entry(entity_id.to_string())
            .or_insert_with(|| Entity {
                id: entity_id.to_string(),
                properties: HashMap::new(),
                last_updated: now,
            });

        // Get old value for delta tracking
        let old_value = entity.properties.get(property).cloned();

        // Update property
        entity.properties.insert(property.to_string(), value.clone());
        entity.last_updated = now;

        // Create state update
        let update = StateUpdate {
            entity_id: entity_id.to_string(),
            property: property.to_string(),
            old_value,
            new_value: value,
            timestamp: now,
        };

        // Broadcast to subscribers (suppressed during NATS replay)
        if !self.replaying.load(Ordering::Relaxed) {
            let _ = self.state_tx.send(update.clone());
        }

        update
    }

    /// Get entity by ID
    pub fn get_entity(&self, entity_id: &str) -> Option<Entity> {
        self.entities.get(entity_id).map(|e| e.clone())
    }

    /// Get all entities
    pub fn get_all_entities(&self) -> Vec<Entity> {
        self.entities.iter().map(|e| e.value().clone()).collect()
    }

    /// Subscribe to state updates
    pub fn subscribe(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }

    /// Subscribe to metrics updates
    pub fn subscribe_metrics(&self) -> broadcast::Receiver<crate::state::metrics_broadcaster::MetricsUpdate> {
        self.metrics_tx.subscribe()
    }

    /// Subscribe to entity deletion events
    pub fn subscribe_deletions(&self) -> broadcast::Receiver<EntityDeleted> {
        self.deletion_tx.subscribe()
    }

    /// Delete entity from state
    pub fn delete_entity(&self, entity_id: &str) -> Option<Entity> {
        // Remove entity from state
        let removed = self.entities.remove(entity_id).map(|(_, entity)| entity);

        if removed.is_some() {
            // Broadcast deletion event (suppressed during NATS replay)
            if !self.replaying.load(Ordering::Relaxed) {
                let deletion = EntityDeleted {
                    entity_id: entity_id.to_string(),
                    timestamp: Utc::now(),
                };
                let _ = self.deletion_tx.send(deletion);
            }

            info!(entity_id = %entity_id, "Entity deleted");
        }

        removed
    }

    /// Get last processed NATS sequence number
    pub fn get_last_processed_sequence(&self) -> u64 {
        self.last_processed_sequence.load(Ordering::SeqCst)
    }

    /// Signal that NATS replay is complete; enable state broadcasting
    pub fn set_live(&self) {
        self.replaying.store(false, Ordering::SeqCst);
        info!("State engine live — broadcasting enabled");
    }

    /// Load state from snapshot
    ///
    /// Clears existing state and loads entities from snapshot.
    /// Sets last_processed_sequence to the snapshot's sequence number.
    pub fn load_from_snapshot(&self, entities: HashMap<String, Entity>, sequence: u64) {
        // Clear existing state
        self.entities.clear();

        // Load entities from snapshot
        for (id, entity) in entities {
            self.entities.insert(id, entity);
        }

        // Set sequence number
        self.last_processed_sequence
            .store(sequence, Ordering::SeqCst);

        info!(
            entities = self.entities.len(),
            sequence = sequence,
            "Loaded state from snapshot"
        );
    }

    /// Process a single event and update state
    ///
    /// Expects payload format:
    /// {
    ///   "entity_id": "...",
    ///   "properties": {
    ///     "prop1": value1,
    ///     "prop2": value2
    ///   }
    /// }
    pub fn process_event(&self, event: &FluxEvent) {
        // Record metrics
        self.metrics.record_event(&event.source);

        // Extract entity_id from payload
        let entity_id = match event.payload.get("entity_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => {
                warn!(
                    event_id = %event.event_id.as_ref().unwrap(),
                    "Event payload missing 'entity_id' field, skipping"
                );
                return;
            }
        };

        // Extract properties object
        let properties = match event.payload.get("properties").and_then(|v| v.as_object()) {
            Some(props) => props,
            None => {
                warn!(
                    event_id = %event.event_id.as_ref().unwrap(),
                    entity_id = %entity_id,
                    "Event payload missing 'properties' object, skipping"
                );
                return;
            }
        };

        // Check for tombstone marker (deletion event)
        if let Some(Value::Bool(true)) = properties.get("__deleted__") {
            self.delete_entity(entity_id);
            return;
        }

        // Update each property
        for (property_name, property_value) in properties {
            self.update_property(entity_id, property_name, property_value.clone());
        }
    }

    /// Determine consumer configuration for NATS event replay.
    ///
    /// Returns `(should_reset, deliver_policy)`:
    /// - `should_reset`: when `true`, the existing durable consumer must be deleted before
    ///   creation. Required when replaying from the beginning (no snapshot), because
    ///   `get_or_create_consumer` silently returns the existing consumer at its current ack
    ///   offset, ignoring the requested `DeliverPolicy`.
    /// - `deliver_policy`: configured delivery start point.
    pub(crate) fn consumer_delivery(
        start_sequence: Option<u64>,
    ) -> (bool, async_nats::jetstream::consumer::DeliverPolicy) {
        match start_sequence {
            None => (true, async_nats::jetstream::consumer::DeliverPolicy::All),
            Some(seq) => (
                false,
                async_nats::jetstream::consumer::DeliverPolicy::ByStartSequence {
                    start_sequence: seq + 1,
                },
            ),
        }
    }

    /// Run NATS subscriber to process events and update state
    ///
    /// This method subscribes to "flux.events.>" and processes all events,
    /// updating in-memory state and broadcasting changes.
    ///
    /// # Arguments
    /// * `start_sequence` - Optional NATS sequence to start from (for recovery).
    ///                      If None, replays all events from the beginning.
    ///                      If Some(n), resumes from n+1 (after snapshot).
    pub async fn run_subscriber(
        self: Arc<Self>,
        jetstream: jetstream::Context,
        start_sequence: Option<u64>,
    ) -> Result<()> {
        info!("Starting state engine NATS subscriber");

        let stream = jetstream
            .get_stream("FLUX_EVENTS")
            .await
            .context("Failed to get FLUX_EVENTS stream")?;

        let (should_reset, deliver_policy) = Self::consumer_delivery(start_sequence);

        let consumer = if should_reset {
            // No snapshot: must replay from the beginning.
            // Delete any existing durable consumer — get_or_create_consumer would silently
            // return it at its current ack offset, ignoring DeliverPolicy::All.
            info!("No snapshot, resetting consumer for full replay from beginning");
            match stream.delete_consumer("flux-state-engine").await {
                Ok(_) => info!("Deleted existing 'flux-state-engine' consumer"),
                Err(e) => info!(error = %e, "No existing consumer to delete (normal on first start)"),
            }
            stream
                .create_consumer(async_nats::jetstream::consumer::pull::Config {
                    durable_name: Some("flux-state-engine".to_string()),
                    filter_subject: "flux.events.>".to_string(),
                    deliver_policy,
                    ..Default::default()
                })
                .await
                .context("Failed to create consumer")?
        } else {
            let seq = start_sequence.unwrap();
            info!(
                start_sequence = seq + 1,
                "Recovering from snapshot, replaying events from sequence {}",
                seq + 1
            );
            stream
                .get_or_create_consumer(
                    "flux-state-engine",
                    async_nats::jetstream::consumer::pull::Config {
                        durable_name: Some("flux-state-engine".to_string()),
                        filter_subject: "flux.events.>".to_string(),
                        deliver_policy,
                        ..Default::default()
                    },
                )
                .await
                .context("Failed to get or create consumer")?
        };

        info!("State engine consumer created, processing events...");

        // Process messages.
        // During replay, use a 500 ms idle timeout: if no message arrives within
        // that window we assume the backlog is drained and we're at the live tail.
        let mut messages = consumer.messages().await?;

        loop {
            let next = if self.replaying.load(Ordering::Relaxed) {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    messages.next(),
                )
                .await
                {
                    Ok(opt) => opt,
                    Err(_) => {
                        // 500 ms elapsed with no message — replay complete
                        self.set_live();
                        messages.next().await
                    }
                }
            } else {
                messages.next().await
            };

            let msg = match next {
                Some(m) => m,
                None => break,
            };

            match msg {
                Ok(msg) => {
                    // Extract NATS sequence number
                    let sequence = match msg.info() {
                        Ok(info) => info.stream_sequence,
                        Err(e) => {
                            error!(error = %e, "Failed to get message info");
                            let _ = msg.ack().await;
                            continue;
                        }
                    };

                    // Deserialize event
                    match serde_json::from_slice::<FluxEvent>(&msg.payload) {
                        Ok(event) => {
                            self.process_event(&event);
                            // Store sequence after successful processing
                            self.last_processed_sequence.store(sequence, Ordering::SeqCst);
                            // Acknowledge message
                            if let Err(e) = msg.ack().await {
                                error!(error = %e, "Failed to acknowledge message");
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to deserialize event, skipping");
                            // Acknowledge to prevent redelivery of malformed messages
                            let _ = msg.ack().await;
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Error receiving message");
                }
            }
        }

        warn!("State engine subscriber stream ended");
        Ok(())
    }
}

impl Default for StateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_event(entity_id: &str, prop: &str, val: serde_json::Value) -> FluxEvent {
        FluxEvent {
            event_id: Some("test-event-id".to_string()),
            stream: "test".to_string(),
            source: "test-source".to_string(),
            timestamp: 1_000_000,
            key: None,
            schema: None,
            payload: json!({
                "entity_id": entity_id,
                "properties": { prop: val }
            }),
        }
    }

    #[test]
    fn broadcast_suppressed_during_replay() {
        let engine = StateEngine::new();
        let mut rx = engine.subscribe();

        // replaying=true by default — broadcast should be suppressed
        let event = make_event("ent/a", "foo", json!(42));
        engine.process_event(&event);

        // Entity state was updated
        assert_eq!(
            engine.get_entity("ent/a").unwrap().properties["foo"],
            json!(42)
        );
        // No broadcast sent
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn broadcast_resumes_after_set_live() {
        let engine = StateEngine::new();
        let mut rx = engine.subscribe();

        engine.set_live();

        let event = make_event("ent/b", "bar", json!("hello"));
        engine.process_event(&event);

        // Broadcast should now be delivered
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn deletion_suppressed_during_replay() {
        let engine = StateEngine::new();
        let mut del_rx = engine.subscribe_deletions();

        // Insert entity first (update_property also suppressed, but entity state is written)
        engine.update_property("ent/c", "x", json!(1));

        // Attempt deletion during replay
        engine.delete_entity("ent/c");

        // Entity removed from state
        assert!(engine.get_entity("ent/c").is_none());
        // No deletion broadcast
        assert!(matches!(
            del_rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn deletion_broadcast_after_set_live() {
        let engine = StateEngine::new();
        engine.set_live();
        let mut del_rx = engine.subscribe_deletions();

        engine.update_property("ent/d", "x", json!(1));
        engine.delete_entity("ent/d");

        assert!(del_rx.try_recv().is_ok());
    }
}
