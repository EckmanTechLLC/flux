use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Runtime-configurable limits. Changes via PUT /api/admin/config take effect immediately
/// without restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub rate_limit_enabled: bool,
    pub rate_limit_per_namespace_per_minute: u64,
    pub body_size_limit_single_bytes: usize,
    pub body_size_limit_batch_bytes: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: true,
            rate_limit_per_namespace_per_minute: 10_000,
            body_size_limit_single_bytes: 1_048_576,   // 1 MB
            body_size_limit_batch_bytes: 10_485_760,   // 10 MB
        }
    }
}

impl RuntimeConfig {
    /// Build from env vars, falling back to defaults.
    pub fn from_env() -> Self {
        let mut cfg = Self::default();

        if let Ok(v) = std::env::var("FLUX_RATE_LIMIT_ENABLED") {
            if let Ok(b) = v.parse::<bool>() {
                cfg.rate_limit_enabled = b;
            }
        }
        if let Ok(v) = std::env::var("FLUX_RATE_LIMIT_PER_NAMESPACE_PER_MINUTE") {
            if let Ok(n) = v.parse::<u64>() {
                cfg.rate_limit_per_namespace_per_minute = n;
            }
        }
        if let Ok(v) = std::env::var("FLUX_BODY_SIZE_LIMIT_SINGLE_BYTES") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.body_size_limit_single_bytes = n;
            }
        }
        if let Ok(v) = std::env::var("FLUX_BODY_SIZE_LIMIT_BATCH_BYTES") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.body_size_limit_batch_bytes = n;
            }
        }

        cfg
    }
}

pub type SharedRuntimeConfig = Arc<RwLock<RuntimeConfig>>;

pub fn new_runtime_config() -> SharedRuntimeConfig {
    Arc::new(RwLock::new(RuntimeConfig::from_env()))
}
