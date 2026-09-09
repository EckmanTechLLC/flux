use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream};
use serde::Deserialize;
use tracing::{debug, error, info, warn};

/// NATS configuration
///
/// Every field carries a serde default so a partial `[nats]` section in config.toml
/// works — previously the section had to be omitted entirely to let `NATS_URL` apply.
#[derive(Clone, Debug, Deserialize)]
pub struct NatsConfig {
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_stream_name")]
    pub stream_name: String,
    #[serde(default = "default_stream_subjects")]
    pub stream_subjects: Vec<String>,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: i64,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: i64,
}

fn default_url() -> String {
    std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string())
}

fn default_stream_name() -> String {
    "FLUX_EVENTS".to_string()
}

fn default_stream_subjects() -> Vec<String> {
    vec!["flux.events.>".to_string()]
}

fn default_max_age_days() -> i64 {
    7
}

/// Stream size ceiling. Deliberately well below any plausible JetStream `max_storage`.
///
/// JetStream derives `max_storage` from free disk at startup and REFUSES to load a
/// stream whose `max_bytes` exceeds it (error 10047), which took the public instance
/// down on 2026-09-09: the disk reached 92%, `max_storage` fell to 9.30 GB, and the
/// stream's 10 GB ceiling became unloadable. Overridable via `FLUX_NATS_MAX_BYTES`.
fn default_max_bytes() -> i64 {
    std::env::var("FLUX_NATS_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(5 * 1024 * 1024 * 1024) // 5 GB
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            stream_name: default_stream_name(),
            stream_subjects: default_stream_subjects(),
            max_age_days: default_max_age_days(),
            max_bytes: default_max_bytes(),
        }
    }
}


/// Turn NATS's opaque storage refusal into something actionable.
///
/// NATS reports this only as "insufficient storage resources available (10047)", naming
/// neither the ceiling nor the limit it exceeded. This condition took the public instance
/// down on 2026-09-09 and cost far more time to diagnose than it should have.
fn storage_failure_hint(err_msg: &str, max_bytes: i64) -> Option<String> {
    let lower = err_msg.to_lowercase();
    if lower.contains("insufficient storage") || lower.contains("10047") {
        Some(format!(
            "NATS refused the stream: its max_bytes ({} bytes / {:.2} GB) exceeds the storage \
             JetStream has available. JetStream derives max_storage from FREE DISK at startup, \
             so this appears when the disk fills. Fix by freeing disk space, or by lowering \
             FLUX_NATS_MAX_BYTES below the server's max_storage (check http://<nats-host>:8222/jsz).",
            max_bytes,
            max_bytes as f64 / 1_073_741_824.0
        ))
    } else {
        None
    }
}

/// NATS client with JetStream
pub struct NatsClient {
    client: async_nats::Client,
    jetstream: jetstream::Context,
    config: NatsConfig,
}

impl NatsClient {
    /// Connect to NATS and initialize JetStream
    pub async fn connect(config: NatsConfig) -> Result<Self> {
        info!("Connecting to NATS at {}", config.url);

        let client = async_nats::connect(&config.url)
            .await
            .context("Failed to connect to NATS")?;

        let jetstream = jetstream::new(client.clone());

        let mut nats_client = Self {
            client,
            jetstream,
            config,
        };

        nats_client.ensure_stream().await?;

        Ok(nats_client)
    }

    /// Ensure the JetStream stream exists AND its config matches what we want.
    ///
    /// Previously this returned early whenever the stream existed, so changes to
    /// `max_bytes` / `max_age_days` silently never applied to a live deployment.
    async fn ensure_stream(&mut self) -> Result<()> {
        info!("Ensuring JetStream stream '{}' exists", self.config.stream_name);

        let desired = stream::Config {
            name: self.config.stream_name.clone(),
            subjects: self.config.stream_subjects.clone(),
            max_age: std::time::Duration::from_secs((self.config.max_age_days * 86400) as u64),
            max_bytes: self.config.max_bytes,
            storage: stream::StorageType::File,
            retention: stream::RetentionPolicy::Limits,
            ..Default::default()
        };

        self.check_storage_headroom(desired.max_bytes).await;

        match self.jetstream.get_stream(&self.config.stream_name).await {
            Ok(mut existing) => {
                let (current_max_bytes, current_max_age) = match existing.info().await {
                    Ok(info) => (info.config.max_bytes, info.config.max_age),
                    Err(e) => {
                        warn!(error = %e, "Could not read stream info; leaving config untouched");
                        return Ok(());
                    }
                };

                if current_max_bytes != desired.max_bytes || current_max_age != desired.max_age {
                    info!(
                        current_max_bytes,
                        desired_max_bytes = desired.max_bytes,
                        current_max_age_secs = current_max_age.as_secs(),
                        desired_max_age_secs = desired.max_age.as_secs(),
                        "Stream config drifted — reconciling"
                    );
                    self.jetstream.update_stream(&desired).await.map_err(|e| {
                        if let Some(hint) = storage_failure_hint(&e.to_string(), desired.max_bytes) {
                            error!("{}", hint);
                        }
                        e
                    })
                    .context("Failed to update existing stream config")?;
                    info!("Stream '{}' config reconciled", self.config.stream_name);
                } else {
                    info!("Stream '{}' exists and config matches", self.config.stream_name);
                }
                Ok(())
            }
            Err(_) => {
                info!("Stream '{}' does not exist, creating...", self.config.stream_name);
                let desired_max_bytes = desired.max_bytes;
                self.jetstream.create_stream(desired).await.map_err(|e| {
                    if let Some(hint) = storage_failure_hint(&e.to_string(), desired_max_bytes) {
                        error!("{}", hint);
                    }
                    e
                })
                .context("Failed to create JetStream stream")?;
                info!("Created JetStream stream '{}'", self.config.stream_name);
                Ok(())
            }
        }
    }

    /// Best-effort headroom check against *account* limits.
    ///
    /// Usually a no-op: the default `$G` account reports empty limits, and the binding
    /// constraint is the SERVER's `max_storage` — derived from free disk and exposed only
    /// on the monitoring port (`/jsz`), not through the client protocol. So this only
    /// fires on deployments that set explicit account limits; everywhere else the real
    /// safety net is `storage_failure_hint` at the point NATS actually refuses.
    async fn check_storage_headroom(&self, max_bytes: i64) {
        match self.jetstream.query_account().await {
            Ok(account) => match account.limits.max_storage {
                Some(max_storage) if max_storage > 0 && max_bytes > max_storage => {
                    error!(
                        stream_max_bytes = max_bytes,
                        account_max_storage = max_storage,
                        "Stream ceiling exceeds this account's max_storage — NATS will refuse \
                         to load the stream. Free disk or lower FLUX_NATS_MAX_BYTES."
                    );
                }
                Some(max_storage) if max_storage > 0 => {
                    info!(
                        stream_max_bytes = max_bytes,
                        account_max_storage = max_storage,
                        headroom_bytes = max_storage - max_bytes,
                        "JetStream account storage headroom OK"
                    );
                }
                _ => {
                    debug!(
                        stream_max_bytes = max_bytes,
                        "Account reports no storage limit; server-level max_storage governs \
                         and is not visible from the client — relying on failure-time diagnosis"
                    );
                }
            },
            Err(e) => {
                warn!(error = %e, "Could not query JetStream account limits");
            }
        }
    }

    /// Get JetStream context for publishing
    pub fn jetstream(&self) -> &jetstream::Context {
        &self.jetstream
    }

    /// Get underlying NATS client
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The verbatim error NATS produced during the 2026-09-09 outage.
    const REAL_OUTAGE_ERROR: &str =
        "jetstream error: 500 (code insufficient storage resources available, error code 10047)";

    #[test]
    fn hint_fires_on_the_real_outage_error() {
        let hint = storage_failure_hint(REAL_OUTAGE_ERROR, 10 * 1024 * 1024 * 1024)
            .expect("must fire on the error that actually took production down");
        assert!(hint.contains("FREE DISK"), "must name the true cause");
        assert!(hint.contains("FLUX_NATS_MAX_BYTES"), "must name the lever");
        assert!(hint.contains("10.00 GB"), "must name the offending ceiling");
    }

    #[test]
    fn hint_fires_on_bare_error_code() {
        assert!(storage_failure_hint("error code 10047", 5).is_some());
    }

    #[test]
    fn hint_stays_quiet_on_unrelated_errors() {
        assert!(storage_failure_hint("stream not found", 5).is_none());
        assert!(storage_failure_hint("connection refused", 5).is_none());
    }

    #[test]
    fn max_bytes_default_is_below_a_plausible_max_storage() {
        // JetStream max_storage fell to 9.30 GB at 92% disk. The default ceiling must
        // sit below that, or a full disk makes the stream unloadable again.
        assert!(
            default_max_bytes() <= 9 * 1024 * 1024 * 1024,
            "default max_bytes must survive the disk pressure that caused the outage"
        );
    }

    #[test]
    fn nats_config_accepts_a_partial_section() {
        // Previously [nats] had to be omitted entirely for NATS_URL to apply.
        let cfg: NatsConfig = toml::from_str("max_age_days = 3").unwrap();
        assert_eq!(cfg.max_age_days, 3);
        assert_eq!(cfg.stream_name, "FLUX_EVENTS");
        assert!(!cfg.url.is_empty());
    }
}
