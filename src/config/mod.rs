pub mod runtime;
pub use runtime::{new_runtime_config, RuntimeConfig, SharedRuntimeConfig};

use serde::Deserialize;

// Re-export existing config types
pub use crate::nats::NatsConfig;
pub use crate::snapshot::config::SnapshotConfig;

/// Complete Flux configuration
#[derive(Debug, Clone, Deserialize)]
pub struct FluxConfig {
    #[serde(default)]
    pub snapshot: SnapshotConfig,
    #[serde(default)]
    pub nats: NatsConfig,
    #[serde(default)]
    pub recovery: RecoveryConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub api: ApiConfig,
}

/// Recovery configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RecoveryConfig {
    #[serde(default = "default_auto_recover")]
    pub auto_recover: bool,
}

fn default_auto_recover() -> bool {
    true
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            auto_recover: default_auto_recover(),
        }
    }
}

/// Metrics configuration (Phase 4A)
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// How often to broadcast metrics via WebSocket (seconds)
    #[serde(default = "default_broadcast_interval")]
    pub broadcast_interval_seconds: u64,
    /// Time window for "active publisher" tracking (seconds)
    #[serde(default = "default_active_publisher_window")]
    pub active_publisher_window_seconds: i64,
}

fn default_broadcast_interval() -> u64 {
    2
}

fn default_active_publisher_window() -> i64 {
    10
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            broadcast_interval_seconds: default_broadcast_interval(),
            active_publisher_window_seconds: default_active_publisher_window(),
        }
    }
}

/// API configuration (Phase 4A)
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// Maximum entities allowed in batch delete operation
    #[serde(default = "default_max_batch_delete")]
    pub max_batch_delete: usize,
}

fn default_max_batch_delete() -> usize {
    10000
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            max_batch_delete: default_max_batch_delete(),
        }
    }
}

impl Default for FluxConfig {
    fn default() -> Self {
        Self {
            snapshot: SnapshotConfig::default(),
            nats: NatsConfig::default(),
            recovery: RecoveryConfig::default(),
            metrics: MetricsConfig::default(),
            api: ApiConfig::default(),
        }
    }
}

/// Load configuration from TOML file
pub fn load_config(path: &str) -> Result<FluxConfig, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(path)?;
    let config: FluxConfig = toml::from_str(&contents)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FluxConfig::default();
        assert_eq!(config.snapshot.enabled, true);
        assert_eq!(config.snapshot.interval_minutes, 5);
        assert_eq!(config.nats.stream_name, "FLUX_EVENTS");
        assert_eq!(config.metrics.broadcast_interval_seconds, 2);
        assert_eq!(config.api.max_batch_delete, 10000);
    }

    #[test]
    fn test_config_deserialization() {
        let toml = r#"
            [snapshot]
            enabled = true
            interval_minutes = 10
            directory = "/tmp/snapshots"
            keep_count = 5

            [nats]
            url = "nats://example.com:4222"
            stream_name = "TEST_STREAM"

            [recovery]
            auto_recover = false

            [metrics]
            broadcast_interval_seconds = 5
            active_publisher_window_seconds = 20

            [api]
            max_batch_delete = 5000
        "#;

        let config: FluxConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.snapshot.interval_minutes, 10);
        assert_eq!(config.nats.url, "nats://example.com:4222");
        assert_eq!(config.recovery.auto_recover, false);
        assert_eq!(config.metrics.broadcast_interval_seconds, 5);
        assert_eq!(config.api.max_batch_delete, 5000);
    }

    #[test]
    fn test_partial_config() {
        // Test that missing sections use defaults
        let toml = r#"
            [metrics]
            broadcast_interval_seconds = 3
        "#;

        let config: FluxConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.metrics.broadcast_interval_seconds, 3);
        assert_eq!(config.snapshot.enabled, true); // Default
        assert_eq!(config.api.max_batch_delete, 10000); // Default
    }
}
