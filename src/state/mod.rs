// State engine and entity management (Task 3)

mod engine;
mod entity;
mod metrics;
mod metrics_broadcaster;

pub use engine::StateEngine;
pub use entity::{Entity, EntityDeleted, StateUpdate};
pub use metrics::{MetricsTracker, MetricsSnapshot};
pub use metrics_broadcaster::{run_metrics_broadcaster, MetricsUpdate};

#[cfg(test)]
mod tests;
