use serde::{Deserialize, Serialize};
use serde_json::Value;

mod validation;
#[cfg(test)]
mod tests;

pub use validation::{validate_and_prepare, ValidationError};

/// FluxEvent represents an immutable event in the Flux system.
///
/// Events have a fixed envelope structure with domain-agnostic payload.
/// All events are time-ordered via UUIDv7 identifiers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FluxEvent {
    /// UUIDv7 identifier (time-ordered, globally unique)
    /// Auto-generated if not provided
    #[serde(rename = "eventId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,

    /// Logical stream/namespace (e.g., "sensors.temperature")
    /// Must be lowercase with optional dot separators
    pub stream: String,

    /// Producer identity (identifies the event source)
    pub source: String,

    /// Unix epoch milliseconds (producer time)
    /// Must be positive
    pub timestamp: i64,

    /// Optional ordering/grouping key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// Optional schema metadata (not validated by Flux)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Domain-specific event data (opaque to Flux)
    /// Must be a valid JSON object
    pub payload: Value,
}

impl FluxEvent {
    /// Validates and prepares an event for ingestion.
    ///
    /// This method:
    /// - Validates required fields
    /// - Validates stream name format
    /// - Validates timestamp is positive
    /// - Validates payload is a JSON object
    /// - Generates UUIDv7 for event_id if missing
    ///
    /// Returns Ok(()) if valid, Err(ValidationError) otherwise.
    pub fn validate_and_prepare(&mut self) -> Result<(), ValidationError> {
        validation::validate_and_prepare(self)
    }
}
