use crate::state::entity::{Entity, StateUpdate};
use chrono::Utc;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

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
}

impl Default for StateEngine {
    fn default() -> Self {
        Self::new()
    }
}
