// NATS client integration (Task 4)

mod client;
mod publisher;

pub use client::{NatsClient, NatsConfig};
pub use publisher::EventPublisher;
