use crate::event::FluxEvent;
use crate::state::entity::{Entity, StateUpdate};
use anyhow::{Context, Result};
use async_nats::jetstream;
use chrono::Utc;
use dashmap::DashMap;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// State engine maintains in-memory world state
pub struct StateEngine {
    /// Lock-free concurrent map for fast reads
    entities: Arc<DashMap<String, Entity>>,

    /// Broadcast channel for state change events
    state_tx: broadcast::Sender<StateUpdate>,
}

impl StateEngine {
    /// Create new state engine with broadcast channel
    pub fn new() -> Self {
        let (state_tx, _) = broadcast::channel(1000);

        Self {
            entities: Arc::new(DashMap::new()),
            state_tx,
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

        // Update each property
        for (property_name, property_value) in properties {
            self.update_property(entity_id, property_name, property_value.clone());
        }
    }

    /// Run NATS subscriber to process events and update state
    ///
    /// This method subscribes to "flux.events.>" and processes all events,
    /// updating in-memory state and broadcasting changes.
    pub async fn run_subscriber(self: Arc<Self>, jetstream: jetstream::Context) -> Result<()> {
        info!("Starting state engine NATS subscriber");

        // Get or create consumer
        let stream = jetstream
            .get_stream("FLUX_EVENTS")
            .await
            .context("Failed to get FLUX_EVENTS stream")?;

        let consumer = stream
            .get_or_create_consumer(
                "flux-state-engine",
                async_nats::jetstream::consumer::pull::Config {
                    durable_name: Some("flux-state-engine".to_string()),
                    filter_subject: "flux.events.>".to_string(),
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
                    // Deserialize event
                    match serde_json::from_slice::<FluxEvent>(&msg.payload) {
                        Ok(event) => {
                            self.process_event(&event);
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
