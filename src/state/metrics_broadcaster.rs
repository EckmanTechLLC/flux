use crate::state::StateEngine;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval, MissedTickBehavior};
use tracing::warn;

/// Periodically broadcast metrics to all subscribers
///
/// This task runs in the background and broadcasts a metrics snapshot
/// every `interval_seconds`. The broadcast is non-blocking and won't
/// affect state engine performance.
pub async fn run_metrics_broadcaster(
    state_engine: Arc<StateEngine>,
    interval_seconds: u64,
    publisher_window_seconds: i64,
) {
    let mut ticker = interval(Duration::from_secs(interval_seconds));

    // Skip missed ticks to prevent backlog under load
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        // Get current entity count (lock-free DashMap operation)
        let entity_count = state_engine.entities.len();

        // Get metrics snapshot
        let metrics_snapshot = state_engine.metrics.get_snapshot(publisher_window_seconds);

        // Create metrics update
        let update = MetricsUpdate {
            entity_count,
            total_events: metrics_snapshot.total_events,
            event_rate: metrics_snapshot.event_rate,
            active_publishers: metrics_snapshot.active_publishers,
            websocket_connections: metrics_snapshot.websocket_connections,
        };

        // Broadcast to all subscribers (ignore send errors - no subscribers is fine)
        if state_engine.metrics_tx.send(update).is_err() {
            // Only log if we've had subscribers before (prevents spam on startup)
            if state_engine.metrics_tx.receiver_count() > 0 {
                warn!("No metrics subscribers available");
            }
        }
    }
}

/// Metrics update message broadcast to WebSocket clients
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsUpdate {
    pub entity_count: usize,
    pub total_events: u64,
    pub event_rate: f64,
    pub active_publishers: usize,
    pub websocket_connections: u64,
}
