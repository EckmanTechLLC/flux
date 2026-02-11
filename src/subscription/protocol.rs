use crate::state::StateUpdate;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Client → Server: Subscribe to entity updates
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename = "subscribe")]
pub struct SubscribeMessage {
    pub entity_id: String,
}

/// Client → Server: Unsubscribe from entity updates
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename = "unsubscribe")]
pub struct UnsubscribeMessage {
    pub entity_id: String,
}

/// Client → Server message types
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe { entity_id: String },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { entity_id: String },
}

/// Server → Client: State update notification
#[derive(Debug, Clone, Serialize)]
pub struct StateUpdateMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub entity_id: String,
    pub property: String,
    pub value: Value,
    pub timestamp: DateTime<Utc>,
}

impl From<StateUpdate> for StateUpdateMessage {
    fn from(update: StateUpdate) -> Self {
        Self {
            msg_type: "state_update".to_string(),
            entity_id: update.entity_id,
            property: update.property,
            value: update.new_value,
            timestamp: update.timestamp,
        }
    }
}

/// Server → Client: Error message
#[derive(Debug, Clone, Serialize)]
pub struct ErrorMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub error: String,
}

impl ErrorMessage {
    pub fn new(error: String) -> Self {
        Self {
            msg_type: "error".to_string(),
            error,
        }
    }
}
