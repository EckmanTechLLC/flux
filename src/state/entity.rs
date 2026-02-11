use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Entity represents a domain-agnostic object in the world state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    /// Unique entity identifier (e.g., "agent_001", "sensor_42")
    pub id: String,

    /// Key-value properties (domain-specific)
    pub properties: HashMap<String, Value>,

    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

/// State update message broadcast to subscribers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateUpdate {
    pub entity_id: String,
    pub property: String,
    pub old_value: Option<Value>,
    pub new_value: Value,
    pub timestamp: DateTime<Utc>,
}
