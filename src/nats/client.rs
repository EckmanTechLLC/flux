use anyhow::{Context, Result};
use async_nats::jetstream::{self, stream};
use serde::Deserialize;
use tracing::info;

/// NATS configuration
#[derive(Clone, Debug, Deserialize)]
pub struct NatsConfig {
    pub url: String,
    pub stream_name: String,
    #[serde(default = "default_stream_subjects")]
    pub stream_subjects: Vec<String>,
    #[serde(default = "default_max_age_days")]
    pub max_age_days: i64,
    #[serde(default = "default_max_bytes")]
    pub max_bytes: i64,
}

fn default_stream_subjects() -> Vec<String> {
    vec!["flux.events.>".to_string()]
}

fn default_max_age_days() -> i64 {
    7
}

fn default_max_bytes() -> i64 {
    10 * 1024 * 1024 * 1024 // 10GB
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            stream_name: "FLUX_EVENTS".to_string(),
            stream_subjects: vec!["flux.events.>".to_string()],
            max_age_days: 7,
            max_bytes: 10 * 1024 * 1024 * 1024, // 10GB
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

    /// Ensure JetStream stream exists with proper configuration
    async fn ensure_stream(&mut self) -> Result<()> {
        info!("Ensuring JetStream stream '{}' exists", self.config.stream_name);

        // Check if stream exists
        match self.jetstream.get_stream(&self.config.stream_name).await {
            Ok(_existing_stream) => {
                info!("Stream '{}' already exists", self.config.stream_name);
                return Ok(());
            }
            Err(_) => {
                info!("Stream '{}' does not exist, creating...", self.config.stream_name);
            }
        }

        // Create stream
        let stream_config = stream::Config {
            name: self.config.stream_name.clone(),
            subjects: self.config.stream_subjects.clone(),
            max_age: std::time::Duration::from_secs((self.config.max_age_days * 86400) as u64),
            max_bytes: self.config.max_bytes,
            storage: stream::StorageType::File,
            retention: stream::RetentionPolicy::Limits,
            ..Default::default()
        };

        self.jetstream
            .create_stream(stream_config)
            .await
            .context("Failed to create JetStream stream")?;

        info!("Created JetStream stream '{}'", self.config.stream_name);
        Ok(())
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
