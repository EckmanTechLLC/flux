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

/// Server → Client: Metrics update notification
#[derive(Debug, Clone, Serialize)]
pub struct MetricsUpdateMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub timestamp: DateTime<Utc>,
    pub entities: MetricsEntityCount,
    pub events: MetricsEvents,
    pub websocket: MetricsWebSocket,
    pub publishers: MetricsPublishers,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsEntityCount {
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsEvents {
    pub total: u64,
    pub rate_per_second: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsWebSocket {
    pub connections: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsPublishers {
    pub active: usize,
}

impl From<crate::state::MetricsUpdate> for MetricsUpdateMessage {
    fn from(update: crate::state::MetricsUpdate) -> Self {
        Self {
            msg_type: "metrics_update".to_string(),
            timestamp: Utc::now(),
            entities: MetricsEntityCount {
                total: update.entity_count,
            },
            events: MetricsEvents {
                total: update.total_events,
                rate_per_second: update.event_rate,
            },
            websocket: MetricsWebSocket {
                connections: update.websocket_connections,
            },
            publishers: MetricsPublishers {
                active: update.active_publishers,
            },
        }
    }
}

/// Server → Client: Entity deleted notification
#[derive(Debug, Clone, Serialize)]
pub struct EntityDeletedMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub entity_id: String,
    pub timestamp: DateTime<Utc>,
}

impl From<crate::state::EntityDeleted> for EntityDeletedMessage {
    fn from(deleted: crate::state::EntityDeleted) -> Self {
        Self {
            msg_type: "entity_deleted".to_string(),
            entity_id: deleted.entity_id,
            timestamp: deleted.timestamp,
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
