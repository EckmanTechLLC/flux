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
use std::sync::atomic::{AtomicU64, Ordering};
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

        // Broadcast to subscribers
        let _ = self.state_tx.send(update.clone());

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
            // Broadcast deletion event
            let deletion = EntityDeleted {
                entity_id: entity_id.to_string(),
                timestamp: Utc::now(),
            };
            let _ = self.deletion_tx.send(deletion);

            info!(entity_id = %entity_id, "Entity deleted");
        }

        removed
    }

    /// Get last processed NATS sequence number
    pub fn get_last_processed_sequence(&self) -> u64 {
        self.last_processed_sequence.load(Ordering::SeqCst)
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

    /// Run NATS subscriber to process events and update state
    ///
    /// This method subscribes to "flux.events.>" and processes all events,
    /// updating in-memory state and broadcasting changes.
    ///
    /// # Arguments
    /// * `start_sequence` - Optional NATS sequence to start from (for recovery).
    ///                      If None, starts from beginning. If Some(n), starts from n+1.
    pub async fn run_subscriber(
        self: Arc<Self>,
        jetstream: jetstream::Context,
        start_sequence: Option<u64>,
    ) -> Result<()> {
        info!("Starting state engine NATS subscriber");

        // Get or create consumer
        let stream = jetstream
            .get_stream("FLUX_EVENTS")
            .await
            .context("Failed to get FLUX_EVENTS stream")?;

        // Configure deliver policy based on start_sequence
        let deliver_policy = match start_sequence {
            Some(seq) => {
                info!(
                    start_sequence = seq + 1,
                    "Recovering from snapshot, replaying events from sequence {}",
                    seq + 1
                );
                async_nats::jetstream::consumer::DeliverPolicy::ByStartSequence {
                    start_sequence: seq + 1,
                }
            }
            None => {
                info!("No snapshot, processing all events from beginning");
                async_nats::jetstream::consumer::DeliverPolicy::All
            }
        };

        let consumer = stream
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
            .context("Failed to create consumer")?;

        info!("State engine consumer created, processing events...");

        // Process messages
        let mut messages = consumer.messages().await?;

        while let Some(msg) = messages.next().await {
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
