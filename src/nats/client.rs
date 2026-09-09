use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream};
use serde::Deserialize;
use tracing::{error, info, warn};

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
                    self.jetstream
                        .update_stream(&desired)
                        .await
                        .context("Failed to update existing stream config")?;
                    info!("Stream '{}' config reconciled", self.config.stream_name);
                } else {
                    info!("Stream '{}' exists and config matches", self.config.stream_name);
                }
                Ok(())
            }
            Err(_) => {
                info!("Stream '{}' does not exist, creating...", self.config.stream_name);
                self.jetstream
                    .create_stream(desired)
                    .await
                    .context("Failed to create JetStream stream")?;
                info!("Created JetStream stream '{}'", self.config.stream_name);
                Ok(())
            }
        }
    }

    /// Warn loudly when the stream ceiling exceeds JetStream's available storage.
    ///
    /// NATS reports this condition only as "insufficient storage resources available
    /// (10047)" with no indication of which number is at fault. Advisory only — it
    /// never blocks startup, since the operator may be mid-cleanup.
    async fn check_storage_headroom(&self, max_bytes: i64) {
        match self.jetstream.query_account().await {
            Ok(account) => match account.limits.max_storage {
                Some(max_storage) if max_storage > 0 && max_bytes > max_storage => {
                    error!(
                        stream_max_bytes = max_bytes,
                        jetstream_max_storage = max_storage,
                        "STREAM CEILING EXCEEDS AVAILABLE JETSTREAM STORAGE. NATS will refuse to \
                         load the stream (error 10047). JetStream derives max_storage from free \
                         disk — free disk space, or lower FLUX_NATS_MAX_BYTES below max_storage."
                    );
                }
                Some(max_storage) if max_storage > 0 => {
                    info!(
                        stream_max_bytes = max_bytes,
                        jetstream_max_storage = max_storage,
                        headroom_bytes = max_storage - max_bytes,
                        "JetStream storage headroom OK"
                    );
                }
                _ => {}
            },
            Err(e) => {
                warn!(error = %e, "Could not query JetStream account limits for preflight check");
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
