use super::FluxEvent;
use std::fmt;
use uuid::Uuid;

/// Validation errors for FluxEvent
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    MissingStream,
    MissingSource,
    MissingPayload,
    InvalidStreamFormat(String),
    InvalidTimestamp(i64),
    PayloadNotObject,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::MissingStream => write!(f, "stream is required"),
            ValidationError::MissingSource => write!(f, "source is required"),
            ValidationError::MissingPayload => write!(f, "payload is required"),
            ValidationError::InvalidStreamFormat(s) => {
                write!(f, "invalid stream format '{}': must be lowercase with optional dots", s)
            }
            ValidationError::InvalidTimestamp(ts) => {
                write!(f, "timestamp must be positive, got {}", ts)
            }
            ValidationError::PayloadNotObject => {
                write!(f, "payload must be a JSON object")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validates and prepares a FluxEvent for ingestion.
///
/// Validation rules:
/// - Required fields: stream, source, timestamp, payload
/// - Stream format: lowercase letters, numbers, dots (e.g., "sensors.temp")
/// - Timestamp: must be positive (Unix epoch milliseconds)
/// - Payload: must be a JSON object (not array, string, etc.)
/// - EventId: auto-generated UUIDv7 if missing or empty
pub fn validate_and_prepare(event: &mut FluxEvent) -> Result<(), ValidationError> {
    // Validate required fields
    if event.stream.is_empty() {
        return Err(ValidationError::MissingStream);
    }
    if event.source.is_empty() {
        return Err(ValidationError::MissingSource);
    }
    if event.payload.is_null() {
        return Err(ValidationError::MissingPayload);
    }

    // Validate stream format (lowercase, numbers, dots)
    if !is_valid_stream_name(&event.stream) {
        return Err(ValidationError::InvalidStreamFormat(event.stream.clone()));
    }

    // Validate timestamp is positive
    if event.timestamp <= 0 {
        return Err(ValidationError::InvalidTimestamp(event.timestamp));
    }

    // Validate payload is an object
    if !event.payload.is_object() {
        return Err(ValidationError::PayloadNotObject);
    }

    // Generate UUIDv7 if missing or empty
    if event.event_id.is_none() || event.event_id.as_ref().map_or(false, |id| id.is_empty()) {
        event.event_id = Some(Uuid::now_v7().to_string());
    }

    Ok(())
}

/// Validates stream name format.
///
/// Valid stream names:
/// - Lowercase letters (a-z)
/// - Numbers (0-9)
/// - Dots (.) for hierarchy
/// - No leading/trailing dots
/// - No consecutive dots
fn is_valid_stream_name(stream: &str) -> bool {
    if stream.is_empty() {
        return false;
    }

    // Check for leading/trailing dots
    if stream.starts_with('.') || stream.ends_with('.') {
        return false;
    }

    // Check for consecutive dots
    if stream.contains("..") {
        return false;
    }

    // Check all characters are valid (lowercase letters, numbers, dots)
    stream.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn test_valid_stream_names() {
        assert!(is_valid_stream_name("sensors"));
        assert!(is_valid_stream_name("sensors.temperature"));
        assert!(is_valid_stream_name("sensors.zone1.temp"));
        assert!(is_valid_stream_name("data123"));
        assert!(is_valid_stream_name("a.b.c.d"));
    }

    #[test]
    fn test_invalid_stream_names() {
        assert!(!is_valid_stream_name(""));
        assert!(!is_valid_stream_name(".sensors"));
        assert!(!is_valid_stream_name("sensors."));
        assert!(!is_valid_stream_name("sensors..temp"));
        assert!(!is_valid_stream_name("Sensors"));
        assert!(!is_valid_stream_name("SENSORS"));
        assert!(!is_valid_stream_name("sensors-temp"));
        assert!(!is_valid_stream_name("sensors_temp"));
        assert!(!is_valid_stream_name("sensors/temp"));
    }
}
