//! i3X connector runner.
//!
//! Manages one long-lived Tokio task per i3X source. Each task:
//! 1. Discovers objects via GET /objects
//! 2. Creates a subscription via POST /subscriptions
//! 3. Registers all objects via POST /subscriptions/{id}/register
//! 4. Streams SSE events from GET /subscriptions/{id}/stream
//! 5. Maps each SSE event → Flux event → POSTs to Flux API
//! 6. Reconnects with exponential backoff on disconnect (1s → 2s → 4s → max 60s)
//! 7. Recreates subscription on 404/410 before reconnecting

use crate::i3x_config::I3xSourceConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// Runtime status for a single i3X source.
#[derive(Clone, Debug, Serialize)]
pub struct I3xStatus {
    pub source_id: String,
    pub source_name: String,
    pub object_count: usize,
    pub last_event: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub restart_count: u32,
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// i3X runner — manages long-lived SSE streaming tasks per source.
pub struct I3xRunner {
    pub flux_api_url: String,
    task_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    status_map: Arc<Mutex<HashMap<String, I3xStatus>>>,
}

impl I3xRunner {
    pub fn new(flux_api_url: String) -> Self {
        Self {
            flux_api_url,
            task_handles: Mutex::new(HashMap::new()),
            status_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts the SSE streaming task for the given i3X source.
    ///
    /// The task runs until `stop_source` is called or the process exits.
    pub async fn start_source(&self, config: &I3xSourceConfig, api_key: String) -> Result<()> {
        {
            let mut map = self.status_map.lock().unwrap();
            map.entry(config.id.clone()).or_insert_with(|| I3xStatus {
                source_id: config.id.clone(),
                source_name: config.name.clone(),
                object_count: 0,
                last_event: None,
                last_error: None,
                restart_count: 0,
            });
        }

        let config_owned = config.clone();
        let flux_url = self.flux_api_url.clone();
        let status_map = Arc::clone(&self.status_map);
        let handle = tokio::spawn(run_i3x_loop(config_owned, api_key, flux_url, status_map));

        let mut handles = self.task_handles.lock().unwrap();
        handles.insert(config.id.clone(), handle);
        info!(source_id = %config.id, name = %config.name, "i3X source started");
        Ok(())
    }

    /// Aborts the streaming task and removes status for the given source.
    pub async fn stop_source(&self, source_id: &str) -> Result<()> {
        let handle = {
            let mut handles = self.task_handles.lock().unwrap();
            handles.remove(source_id)
        };
        if let Some(h) = handle {
            h.abort();
        }
        {
            let mut map = self.status_map.lock().unwrap();
            map.remove(source_id);
        }
        info!(source_id = %source_id, "i3X source stopped");
        Ok(())
    }

    /// Returns current status for all i3X sources.
    pub fn status(&self) -> Vec<I3xStatus> {
        let map = self.status_map.lock().unwrap();
        map.values().cloned().collect()
    }

    /// Triggers a one-shot sync for the given i3X source.
    ///
    /// Creates a fresh subscription, registers all objects, calls
    /// `POST /subscriptions/{id}/sync`, then exits. Fire-and-forget.
    pub async fn trigger_sync(&self, config: &I3xSourceConfig, api_key: String) -> Result<()> {
        {
            let mut map = self.status_map.lock().unwrap();
            map.entry(config.id.clone()).or_insert_with(|| I3xStatus {
                source_id: config.id.clone(),
                source_name: config.name.clone(),
                object_count: 0,
                last_event: None,
                last_error: None,
                restart_count: 0,
            });
        }
        let config_owned = config.clone();
        let status_map = Arc::clone(&self.status_map);
        tokio::spawn(async move {
            if let Err(e) = sync_once_http(&config_owned, &api_key, &status_map).await {
                warn!(source_id = %config_owned.id, error = %e, "i3X one-shot sync failed");
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config_owned.id) {
                    s.last_error = Some(e.to_string());
                }
            }
        });
        info!(source_id = %config.id, "i3X one-shot sync triggered");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SSE loop
// ---------------------------------------------------------------------------

/// Outer loop: attempts to connect, reconnects with exponential backoff on failure.
async fn run_i3x_loop(
    config: I3xSourceConfig,
    api_key: String,
    flux_api_url: String,
    status_map: Arc<Mutex<HashMap<String, I3xStatus>>>,
) {
    let mut backoff_secs: u64 = 1;

    loop {
        info!(source_id = %config.id, "i3X connection attempt");

        match stream_once(&config, &api_key, &flux_api_url, &status_map).await {
            Ok(()) => {
                // Stream ended cleanly (server closed connection) — reconnect immediately.
                info!(source_id = %config.id, "i3X SSE stream ended cleanly, reconnecting");
                backoff_secs = 1;
            }
            Err(e) => {
                warn!(
                    source_id = %config.id,
                    error = %e,
                    backoff_secs,
                    "i3X stream error"
                );
                {
                    let mut map = status_map.lock().unwrap();
                    if let Some(s) = map.get_mut(&config.id) {
                        s.last_error = Some(e.to_string());
                        s.restart_count += 1;
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
            }
        }
    }
}

/// One streaming session: discover → subscribe → register → stream SSE.
///
/// Returns `Ok(())` when the stream ends cleanly.
/// Returns `Err` on any setup or streaming error (triggers reconnect with backoff).
async fn stream_once(
    config: &I3xSourceConfig,
    api_key: &str,
    flux_api_url: &str,
    status_map: &Arc<Mutex<HashMap<String, I3xStatus>>>,
) -> Result<()> {
    // Standard client with timeout for setup calls.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Step 1: Discover objects.
    let objects = discover_objects(&client, &config.base_url, api_key)
        .await
        .context("Failed to discover i3X objects")?;
    let object_count = objects.len();
    info!(source_id = %config.id, count = object_count, "i3X objects discovered");

    {
        let mut map = status_map.lock().unwrap();
        if let Some(s) = map.get_mut(&config.id) {
            s.object_count = object_count;
            s.last_error = None;
        }
    }

    // Step 2: Create subscription.
    let subscription_id = create_subscription(&client, &config.base_url, api_key)
        .await
        .context("Failed to create i3X subscription")?;
    info!(
        source_id = %config.id,
        subscription_id = %subscription_id,
        "i3X subscription created"
    );

    // Step 3: Register all objects.
    if !objects.is_empty() {
        register_objects(&client, &config.base_url, api_key, &subscription_id, &objects)
            .await
            .context("Failed to register i3X objects")?;
        info!(source_id = %config.id, count = object_count, "i3X objects registered");
    }

    // Step 4: Open SSE stream (no timeout — long-lived connection).
    let sse_client = reqwest::Client::builder().build()?;
    let stream_url = format!(
        "{}/subscriptions/{}/stream",
        config.base_url, subscription_id
    );

    let mut response = sse_client
        .get(&stream_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "text/event-stream")
        .send()
        .await
        .context("Failed to connect to i3X SSE stream")?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
        return Err(anyhow::anyhow!(
            "i3X subscription {} expired or not found ({})",
            subscription_id,
            status
        ));
    }
    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "i3X SSE stream returned error status {}",
            status
        ));
    }

    // Flux client for publishing events (10s timeout per request).
    let flux_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Step 5: Read SSE stream chunk by chunk.
    let mut buffer = String::new();

    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let text = match std::str::from_utf8(&chunk) {
                    Ok(t) => t,
                    Err(e) => {
                        warn!(source_id = %config.id, error = %e, "SSE chunk is not valid UTF-8, skipping");
                        continue;
                    }
                };
                buffer.push_str(text);

                // Process all complete lines in the buffer.
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer.drain(..=pos);

                    if !line.starts_with("data:") {
                        // Ignore "event:", "id:", comments, and blank lines.
                        continue;
                    }

                    let json_str = line["data:".len()..].trim();
                    if json_str.is_empty() {
                        continue;
                    }

                    match process_sse_event(json_str, config, &flux_client, flux_api_url).await {
                        Ok(()) => {
                            let mut map = status_map.lock().unwrap();
                            if let Some(s) = map.get_mut(&config.id) {
                                s.last_event = Some(Utc::now());
                                s.last_error = None;
                            }
                        }
                        Err(e) => {
                            warn!(
                                source_id = %config.id,
                                error = %e,
                                "Failed to process i3X SSE event"
                            );
                            let mut map = status_map.lock().unwrap();
                            if let Some(s) = map.get_mut(&config.id) {
                                s.last_error = Some(e.to_string());
                            }
                        }
                    }
                }
            }
            Ok(None) => {
                // Server closed the connection cleanly.
                return Ok(());
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// i3X API helpers
// ---------------------------------------------------------------------------

/// One object returned by GET /objects.
#[derive(Debug, Deserialize)]
struct I3xObject {
    #[serde(rename = "elementId")]
    element_id: String,
}

/// Response body from POST /subscriptions.
#[derive(Debug, Deserialize)]
struct SubscriptionResponse {
    id: String,
}

/// GET /objects → returns all element IDs.
async fn discover_objects(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>> {
    let resp = client
        .get(format!("{}/objects", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("GET /objects request failed")?
        .error_for_status()
        .context("GET /objects returned error status")?;

    let objects: Vec<I3xObject> = resp
        .json()
        .await
        .context("Failed to parse /objects response")?;

    Ok(objects.into_iter().map(|o| o.element_id).collect())
}

/// POST /subscriptions → returns the new subscription ID.
async fn create_subscription(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<String> {
    let resp = client
        .post(format!("{}/subscriptions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({}))
        .send()
        .await
        .context("POST /subscriptions request failed")?
        .error_for_status()
        .context("POST /subscriptions returned error status")?;

    let sub: SubscriptionResponse = resp
        .json()
        .await
        .context("Failed to parse subscription response")?;

    Ok(sub.id)
}

/// POST /subscriptions/{id}/register — registers element IDs for monitoring.
async fn register_objects(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    subscription_id: &str,
    element_ids: &[String],
) -> Result<()> {
    client
        .post(format!(
            "{}/subscriptions/{}/register",
            base_url, subscription_id
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&serde_json::json!({ "elementIds": element_ids }))
        .send()
        .await
        .context("POST /subscriptions/{id}/register request failed")?
        .error_for_status()
        .context("POST /subscriptions/{id}/register returned error status")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// SSE event processing
// ---------------------------------------------------------------------------

/// Parsed SSE event from the i3X stream.
#[derive(Debug, Deserialize)]
struct SseEvent {
    #[serde(rename = "elementId")]
    element_id: String,
    value: serde_json::Value,
    quality: Option<String>,
    timestamp: Option<String>,
}

/// Parses one SSE `data:` JSON payload and publishes a Flux event.
async fn process_sse_event(
    json_str: &str,
    config: &I3xSourceConfig,
    flux_client: &reqwest::Client,
    flux_api_url: &str,
) -> Result<()> {
    let event: SseEvent = serde_json::from_str(json_str)
        .with_context(|| format!("Failed to parse SSE event JSON: {}", json_str))?;

    let entity_id = format!(
        "{}/{}",
        config.namespace,
        sanitize_element_id(&event.element_id)
    );
    let quality = event.quality.unwrap_or_else(|| "Unknown".to_string());
    let timestamp = event.timestamp.unwrap_or_default();

    // Build properties: spread object keys or wrap scalar under "value".
    let properties = build_properties(event.value, &quality, &timestamp);

    let flux_event = serde_json::json!({
        "stream": "i3x",
        "source": format!("i3x.{}", config.id),
        "timestamp": Utc::now().timestamp_millis(),
        "key": event.element_id,
        "payload": {
            "entity_id": entity_id,
            "properties": properties,
        }
    });

    let mut req = flux_client
        .post(format!("{}/api/events", flux_api_url))
        .json(&flux_event);

    if !config.flux_namespace_token.is_empty() {
        req = req.header(
            "Authorization",
            format!("Bearer {}", config.flux_namespace_token),
        );
    }

    req.send()
        .await
        .context("Failed to POST i3X event to Flux")?
        .error_for_status()
        .context("Flux API returned error status for i3X event")?;

    Ok(())
}

/// Builds the Flux properties map from an i3X value + quality + timestamp.
///
/// - Object values: each key becomes a property (spread).
/// - Scalar values (number, string, bool, null): wrapped under `"value"` key.
/// - `i3x_quality` and `i3x_timestamp` are always appended.
fn build_properties(
    value: serde_json::Value,
    quality: &str,
    timestamp: &str,
) -> serde_json::Value {
    let mut map = match value {
        serde_json::Value::Object(m) => m,
        scalar => {
            let mut m = serde_json::Map::new();
            m.insert("value".to_string(), scalar);
            m
        }
    };
    map.insert(
        "i3x_quality".to_string(),
        serde_json::Value::String(quality.to_string()),
    );
    map.insert(
        "i3x_timestamp".to_string(),
        serde_json::Value::String(timestamp.to_string()),
    );
    serde_json::Value::Object(map)
}

/// Sanitizes an i3X elementId for use as a Flux entity key segment.
///
/// Replaces any character that is not alphanumeric, `-`, `_`, or `.` with `_`.
fn sanitize_element_id(element_id: &str) -> String {
    element_id
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// One-shot sync
// ---------------------------------------------------------------------------

/// Creates a fresh subscription, registers objects, calls the i3X sync endpoint.
///
/// The `/sync` response format is not specified in the i3X OpenAPI spec at time
/// of writing, so we verify the status code and discard the body.
async fn sync_once_http(
    config: &I3xSourceConfig,
    api_key: &str,
    status_map: &Arc<Mutex<HashMap<String, I3xStatus>>>,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let objects = discover_objects(&client, &config.base_url, api_key)
        .await
        .context("Failed to discover i3X objects for sync")?;

    {
        let mut map = status_map.lock().unwrap();
        if let Some(s) = map.get_mut(&config.id) {
            s.object_count = objects.len();
        }
    }

    let subscription_id = create_subscription(&client, &config.base_url, api_key)
        .await
        .context("Failed to create i3X subscription for sync")?;

    if !objects.is_empty() {
        register_objects(&client, &config.base_url, api_key, &subscription_id, &objects)
            .await
            .context("Failed to register i3X objects for sync")?;
    }

    client
        .post(format!(
            "{}/subscriptions/{}/sync",
            config.base_url, subscription_id
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("POST /subscriptions/{id}/sync failed")?
        .error_for_status()
        .context("i3X sync endpoint returned error status")?;

    {
        let mut map = status_map.lock().unwrap();
        if let Some(s) = map.get_mut(&config.id) {
            s.last_event = Some(Utc::now());
            s.last_error = None;
        }
    }

    info!(source_id = %config.id, "i3X one-shot sync complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_element_id_passthrough() {
        assert_eq!(sanitize_element_id("pump.speed-1_a"), "pump.speed-1_a");
        assert_eq!(sanitize_element_id("ABC123"), "ABC123");
    }

    #[test]
    fn test_sanitize_element_id_replaces_slashes_and_spaces() {
        assert_eq!(sanitize_element_id("pump/speed"), "pump_speed");
        assert_eq!(sanitize_element_id("has spaces"), "has_spaces");
        assert_eq!(sanitize_element_id("a:b:c"), "a_b_c");
    }

    #[test]
    fn test_parse_sse_scalar_value() {
        let json = r#"{"elementId":"pump.speed","value":42.5,"quality":"Good","timestamp":"2026-03-13T10:00:00Z"}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.element_id, "pump.speed");
        assert_eq!(event.value, serde_json::json!(42.5));
        assert_eq!(event.quality.unwrap(), "Good");
        assert_eq!(event.timestamp.unwrap(), "2026-03-13T10:00:00Z");
    }

    #[test]
    fn test_parse_sse_object_value() {
        let json = r#"{"elementId":"sensor.1","value":{"temp":72.3,"pressure":14.7},"quality":"Good","timestamp":"2026-03-13T10:00:00Z"}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.element_id, "sensor.1");
        let obj = event.value.as_object().unwrap();
        assert_eq!(obj["temp"], serde_json::json!(72.3));
        assert_eq!(obj["pressure"], serde_json::json!(14.7));
    }

    #[test]
    fn test_parse_sse_missing_optional_fields() {
        let json = r#"{"elementId":"flow.meter","value":100}"#;
        let event: SseEvent = serde_json::from_str(json).unwrap();
        assert!(event.quality.is_none());
        assert!(event.timestamp.is_none());
    }

    #[test]
    fn test_build_properties_scalar() {
        let props = build_properties(
            serde_json::json!(42.5),
            "Good",
            "2026-03-13T10:00:00Z",
        );
        let obj = props.as_object().unwrap();
        assert_eq!(obj["value"], serde_json::json!(42.5));
        assert_eq!(obj["i3x_quality"], "Good");
        assert_eq!(obj["i3x_timestamp"], "2026-03-13T10:00:00Z");
        assert!(!obj.contains_key("elementId"));
    }

    #[test]
    fn test_build_properties_object_spread() {
        let props = build_properties(
            serde_json::json!({"temp": 72.3, "pressure": 14.7}),
            "Good",
            "2026-03-13T10:00:00Z",
        );
        let obj = props.as_object().unwrap();
        assert_eq!(obj["temp"], serde_json::json!(72.3));
        assert_eq!(obj["pressure"], serde_json::json!(14.7));
        assert_eq!(obj["i3x_quality"], "Good");
        // No "value" wrapper for object values.
        assert!(!obj.contains_key("value"));
    }

    #[test]
    fn test_build_properties_null_value() {
        let props = build_properties(serde_json::Value::Null, "Bad", "");
        let obj = props.as_object().unwrap();
        assert_eq!(obj["value"], serde_json::Value::Null);
        assert_eq!(obj["i3x_quality"], "Bad");
    }

    #[test]
    fn test_build_properties_string_value() {
        let props = build_properties(serde_json::json!("running"), "Good", "ts");
        let obj = props.as_object().unwrap();
        assert_eq!(obj["value"], "running");
    }

    #[test]
    fn test_i3x_runner_status_empty() {
        let runner = I3xRunner::new("http://localhost:3000".to_string());
        assert!(runner.status().is_empty());
    }

    #[test]
    fn test_i3x_status_serializes() {
        let status = I3xStatus {
            source_id: "src-1".to_string(),
            source_name: "Test".to_string(),
            object_count: 5,
            last_event: None,
            last_error: None,
            restart_count: 0,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("src-1"));
        assert!(json.contains("object_count"));
    }
}
