use crate::event::FluxEvent;
use anyhow::{Context, Result};
use async_nats::jetstream;
use tracing::debug;

/// Event publisher for NATS JetStream
#[derive(Clone)]
pub struct EventPublisher {
    jetstream: jetstream::Context,
}

impl EventPublisher {
    /// Create a new event publisher
    pub fn new(jetstream: jetstream::Context) -> Self {
        Self { jetstream }
    }

    /// Publish a single event to NATS
    ///
    /// Subject format: flux.events.{stream}
    /// Payload: JSON-serialized FluxEvent
    pub async fn publish(&self, event: &FluxEvent) -> Result<()> {
        let subject = format!("flux.events.{}", event.stream);
        let payload = serde_json::to_vec(event)
            .context("Failed to serialize event to JSON")?;

        debug!(
            event_id = %event.event_id.as_ref().unwrap(),
            stream = %event.stream,
            subject = %subject,
            "Publishing event to NATS"
        );

        self.jetstream
            .publish(subject.clone(), payload.into())
            .await
            .context(format!("Failed to publish event to subject '{}'", subject))?
            .await
            .context("Failed to await publish ack")?;

        Ok(())
    }

    /// Publish multiple events in batch
    pub async fn publish_batch(&self, events: &[FluxEvent]) -> Result<Vec<Result<()>>> {
        let mut results = Vec::with_capacity(events.len());

        for event in events {
            let result = self.publish(event).await;
            results.push(result);
        }

        Ok(results)
    }
}
