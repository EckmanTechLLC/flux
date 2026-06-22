use crate::state::StateUpdate;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── QoS Tiers (SP-inspired) ───────────────────────────────────────────────

/// Quality-of-service tier for subscriptions.
///
/// Inspired by SingularisPrime's `QoS { best_effort, at_least_once, exactly_once }`,
/// adapted to Flux's WebSocket + broadcast channel architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QosTier {
    /// Drop on lag, lowest latency. Default behavior.
    #[default]
    Realtime,
    /// Buffer messages during lag, deliver catch-up batch on resume.
    Reliable,
    /// Only deliver latest entity state on (re)connect — poll mode.
    Snapshot,
}

// ─── Client → Server Messages ──────────────────────────────────────────────

/// Client → Server: Subscribe to entity updates
///
/// Extended with SingularisPrime-inspired filtering:
/// - `entity_id`: exact ID, trailing `*` for prefix match, or `*` for all
/// - `properties`: optional filter — only forward updates for listed properties
/// - `qos`: delivery guarantee tier
/// - `delta`: if true, send numeric deltas instead of absolute values
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "subscribe")]
    Subscribe {
        entity_id: String,
        /// Optional: only forward updates for these property names
        #[serde(default)]
        properties: Option<Vec<String>>,
        /// Quality-of-service tier (default: realtime)
        #[serde(default)]
        qos: QosTier,
        /// Enable delta compression for numeric values
        #[serde(default)]
        delta: bool,
    },
    #[serde(rename = "unsubscribe")]
    Unsubscribe {
        entity_id: String,
    },
}

/// Parsed subscription entry stored per-connection
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Original pattern from client
    pub pattern: String,
    /// Matching mode derived from pattern
    pub match_mode: MatchMode,
    /// Optional property filter set
    pub properties: Option<Vec<String>>,
    /// QoS tier
    pub qos: QosTier,
    /// Delta compression enabled
    pub delta: bool,
}

/// How to match entity IDs against this subscription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchMode {
    /// Match all entities (`*`)
    Wildcard,
    /// Match entities whose ID starts with prefix (pattern ends with `*`)
    Prefix(String),
    /// Match exact entity ID
    Exact(String),
}

impl Subscription {
    /// Parse a subscription from a client subscribe message
    pub fn from_subscribe(
        entity_id: &str,
        properties: Option<Vec<String>>,
        qos: QosTier,
        delta: bool,
    ) -> Self {
        let match_mode = if entity_id == "*" {
            MatchMode::Wildcard
        } else if entity_id.ends_with('*') {
            MatchMode::Prefix(entity_id[..entity_id.len() - 1].to_string())
        } else {
            MatchMode::Exact(entity_id.to_string())
        };

        Self {
            pattern: entity_id.to_string(),
            match_mode,
            properties,
            qos,
            delta,
        }
    }

    /// Check if an entity ID matches this subscription
    pub fn matches_entity(&self, entity_id: &str) -> bool {
        match &self.match_mode {
            MatchMode::Wildcard => true,
            MatchMode::Prefix(prefix) => entity_id.starts_with(prefix),
            MatchMode::Exact(id) => entity_id == id,
        }
    }

    /// Check if a property name passes this subscription's filter
    pub fn matches_property(&self, property: &str) -> bool {
        match &self.properties {
            None => true,
            Some(props) => props.iter().any(|p| p == property),
        }
    }

    /// Check if a state update should be forwarded to this subscription
    pub fn matches_update(&self, update: &StateUpdate) -> bool {
        self.matches_entity(&update.entity_id) && self.matches_property(&update.property)
    }
}

// ─── Server → Client Messages ──────────────────────────────────────────────

/// Server → Client: State update notification (absolute value)
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

/// Server → Client: Delta update notification (numeric difference)
///
/// Sent instead of StateUpdateMessage when the subscription has `delta: true`
/// and the value change is numeric.
#[derive(Debug, Clone, Serialize)]
pub struct StateDeltaMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub entity_id: String,
    pub property: String,
    pub delta: f64,
    pub timestamp: DateTime<Utc>,
}

impl StateDeltaMessage {
    /// Try to create a delta message from a state update.
    /// Returns None if old_value is missing or either value is non-numeric.
    pub fn try_from_update(update: &StateUpdate) -> Option<Self> {
        let old = update.old_value.as_ref()?;
        let old_f = as_f64(old)?;
        let new_f = as_f64(&update.new_value)?;
        Some(Self {
            msg_type: "state_delta".to_string(),
            entity_id: update.entity_id.clone(),
            property: update.property.clone(),
            delta: new_f - old_f,
            timestamp: update.timestamp,
        })
    }
}

/// Extract f64 from a JSON value (handles both integer and float)
fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        _ => None,
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

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_wildcard_subscription() {
        let sub = Subscription::from_subscribe("*", None, QosTier::Realtime, false);
        assert_eq!(sub.match_mode, MatchMode::Wildcard);
        assert!(sub.matches_entity("anything"));
        assert!(sub.matches_entity("pure-jade/scada-tr-01"));
    }

    #[test]
    fn test_prefix_subscription() {
        let sub = Subscription::from_subscribe("pure-jade/scada-*", None, QosTier::Realtime, false);
        assert_eq!(sub.match_mode, MatchMode::Prefix("pure-jade/scada-".to_string()));
        assert!(sub.matches_entity("pure-jade/scada-tr-01"));
        assert!(sub.matches_entity("pure-jade/scada-bk-feeder-01"));
        assert!(!sub.matches_entity("pure-ash/arc-01"));
        assert!(!sub.matches_entity("pure-jade/kannaka-01"));
    }

    #[test]
    fn test_exact_subscription() {
        let sub = Subscription::from_subscribe("pure-jade/kannaka-01", None, QosTier::Realtime, false);
        assert_eq!(sub.match_mode, MatchMode::Exact("pure-jade/kannaka-01".to_string()));
        assert!(sub.matches_entity("pure-jade/kannaka-01"));
        assert!(!sub.matches_entity("pure-jade/kannaka-02"));
    }

    #[test]
    fn test_property_filter() {
        let sub = Subscription::from_subscribe(
            "*",
            Some(vec!["status".to_string(), "current".to_string()]),
            QosTier::Realtime,
            false,
        );
        assert!(sub.matches_property("status"));
        assert!(sub.matches_property("current"));
        assert!(!sub.matches_property("voltage"));
    }

    #[test]
    fn test_property_filter_none_matches_all() {
        let sub = Subscription::from_subscribe("*", None, QosTier::Realtime, false);
        assert!(sub.matches_property("anything"));
    }

    #[test]
    fn test_delta_message_numeric() {
        let update = StateUpdate {
            entity_id: "test".to_string(),
            property: "current".to_string(),
            old_value: Some(json!(100)),
            new_value: json!(112),
            timestamp: Utc::now(),
        };
        let delta = StateDeltaMessage::try_from_update(&update).unwrap();
        assert!((delta.delta - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_delta_message_non_numeric() {
        let update = StateUpdate {
            entity_id: "test".to_string(),
            property: "status".to_string(),
            old_value: Some(json!("online")),
            new_value: json!("offline"),
            timestamp: Utc::now(),
        };
        assert!(StateDeltaMessage::try_from_update(&update).is_none());
    }

    #[test]
    fn test_delta_message_no_old_value() {
        let update = StateUpdate {
            entity_id: "test".to_string(),
            property: "current".to_string(),
            old_value: None,
            new_value: json!(100),
            timestamp: Utc::now(),
        };
        assert!(StateDeltaMessage::try_from_update(&update).is_none());
    }

    #[test]
    fn test_deserialize_subscribe_minimal() {
        let msg: ClientMessage = serde_json::from_str(
            r#"{"type":"subscribe","entity_id":"*"}"#,
        ).unwrap();
        match msg {
            ClientMessage::Subscribe { entity_id, properties, qos, delta } => {
                assert_eq!(entity_id, "*");
                assert!(properties.is_none());
                assert_eq!(qos, QosTier::Realtime);
                assert!(!delta);
            }
            _ => panic!("expected Subscribe"),
        }
    }

    #[test]
    fn test_deserialize_subscribe_full() {
        let msg: ClientMessage = serde_json::from_str(
            r#"{"type":"subscribe","entity_id":"pure-jade/scada-*","properties":["status","current"],"qos":"reliable","delta":true}"#,
        ).unwrap();
        match msg {
            ClientMessage::Subscribe { entity_id, properties, qos, delta } => {
                assert_eq!(entity_id, "pure-jade/scada-*");
                assert_eq!(properties.unwrap(), vec!["status", "current"]);
                assert_eq!(qos, QosTier::Reliable);
                assert!(delta);
            }
            _ => panic!("expected Subscribe"),
        }
    }
}
