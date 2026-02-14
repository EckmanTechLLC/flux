use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use chrono::Utc;
use serde::Serialize;

/// Tracks metrics for the Flux state engine
#[derive(Clone)]
pub struct MetricsTracker {
    /// Total events processed (lifetime counter)
    total_events: Arc<AtomicU64>,

    /// Event timestamps for rate calculation (sliding 5-second window)
    event_timestamps: Arc<RwLock<VecDeque<i64>>>,

    /// Active publishers (source -> last_seen_timestamp_ms)
    active_publishers: Arc<RwLock<HashMap<String, i64>>>,

    /// WebSocket connection count
    websocket_connections: Arc<AtomicU64>,
}

impl MetricsTracker {
    /// Create new metrics tracker
    pub fn new() -> Self {
        Self {
            total_events: Arc::new(AtomicU64::new(0)),
            event_timestamps: Arc::new(RwLock::new(VecDeque::new())),
            active_publishers: Arc::new(RwLock::new(HashMap::new())),
            websocket_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record an event (call from StateEngine.process_event)
    pub fn record_event(&self, source: &str) {
        // Increment total counter
        self.total_events.fetch_add(1, Ordering::Relaxed);

        let now = Utc::now().timestamp_millis();

        // Update sliding window for rate calculation
        {
            let mut timestamps = self.event_timestamps.write().unwrap();
            timestamps.push_back(now);

            // Prune old timestamps (keep last 5 seconds)
            while let Some(&oldest) = timestamps.front() {
                if now - oldest > 5000 {
                    timestamps.pop_front();
                } else {
                    break;
                }
            }
        }

        // Update active publishers
        {
            let mut publishers = self.active_publishers.write().unwrap();
            publishers.insert(source.to_string(), now);
        }
    }

    /// Get current event rate (events per second over last 5 seconds)
    pub fn get_event_rate(&self) -> f64 {
        let timestamps = self.event_timestamps.read().unwrap();
        timestamps.len() as f64 / 5.0
    }

    /// Get count of active publishers (published within window)
    pub fn get_active_publisher_count(&self, window_seconds: i64) -> usize {
        let now = Utc::now().timestamp_millis();
        let threshold = now - (window_seconds * 1000);

        let publishers = self.active_publishers.read().unwrap();
        publishers
            .values()
            .filter(|&&last_seen| last_seen > threshold)
            .count()
    }

    /// Increment WebSocket connection count
    pub fn increment_ws_connection(&self) {
        self.websocket_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement WebSocket connection count
    pub fn decrement_ws_connection(&self) {
        self.websocket_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current WebSocket connection count
    pub fn get_ws_connection_count(&self) -> u64 {
        self.websocket_connections.load(Ordering::Relaxed)
    }

    /// Get total events processed
    pub fn get_total_events(&self) -> u64 {
        self.total_events.load(Ordering::Relaxed)
    }

    /// Get snapshot of all metrics
    pub fn get_snapshot(&self, publisher_window_seconds: i64) -> MetricsSnapshot {
        MetricsSnapshot {
            total_events: self.get_total_events(),
            event_rate: self.get_event_rate(),
            active_publishers: self.get_active_publisher_count(publisher_window_seconds),
            websocket_connections: self.get_ws_connection_count(),
        }
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub total_events: u64,
    pub event_rate: f64,
    pub active_publishers: usize,
    pub websocket_connections: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_event_recording() {
        let tracker = MetricsTracker::new();

        assert_eq!(tracker.get_total_events(), 0);

        tracker.record_event("source1");
        assert_eq!(tracker.get_total_events(), 1);

        tracker.record_event("source2");
        assert_eq!(tracker.get_total_events(), 2);
    }

    #[test]
    fn test_event_rate_calculation() {
        let tracker = MetricsTracker::new();

        // Record 10 events
        for _ in 0..10 {
            tracker.record_event("source1");
        }

        // Rate should be ~10/5 = 2 events per second
        let rate = tracker.get_event_rate();
        assert_eq!(rate, 2.0);
    }

    #[test]
    fn test_sliding_window_cleanup() {
        let tracker = MetricsTracker::new();

        // Record an event
        tracker.record_event("source1");
        assert_eq!(tracker.get_event_rate(), 0.2); // 1 event / 5s

        // Sleep 6 seconds (longer than window)
        thread::sleep(Duration::from_secs(6));

        // Record a new event to trigger cleanup
        tracker.record_event("source2");

        // Old event should be pruned, only new event remains
        assert_eq!(tracker.get_event_rate(), 0.2); // 1 event / 5s
    }

    #[test]
    fn test_active_publisher_tracking() {
        let tracker = MetricsTracker::new();

        tracker.record_event("source1");
        tracker.record_event("source2");
        tracker.record_event("source1"); // Duplicate source

        // Should have 2 unique publishers
        assert_eq!(tracker.get_active_publisher_count(10), 2);
    }

    #[test]
    fn test_active_publisher_window() {
        let tracker = MetricsTracker::new();

        tracker.record_event("source1");
        thread::sleep(Duration::from_secs(2));
        tracker.record_event("source2");

        // With 10s window, both should be active
        assert_eq!(tracker.get_active_publisher_count(10), 2);

        // With 1s window, only source2 should be active
        assert_eq!(tracker.get_active_publisher_count(1), 1);
    }

    #[test]
    fn test_websocket_connection_tracking() {
        let tracker = MetricsTracker::new();

        assert_eq!(tracker.get_ws_connection_count(), 0);

        tracker.increment_ws_connection();
        assert_eq!(tracker.get_ws_connection_count(), 1);

        tracker.increment_ws_connection();
        assert_eq!(tracker.get_ws_connection_count(), 2);

        tracker.decrement_ws_connection();
        assert_eq!(tracker.get_ws_connection_count(), 1);
    }

    #[test]
    fn test_metrics_snapshot() {
        let tracker = MetricsTracker::new();

        tracker.record_event("source1");
        tracker.record_event("source2");
        tracker.increment_ws_connection();

        let snapshot = tracker.get_snapshot(10);

        assert_eq!(snapshot.total_events, 2);
        assert_eq!(snapshot.active_publishers, 2);
        assert_eq!(snapshot.websocket_connections, 1);
        assert!(snapshot.event_rate > 0.0);
    }

    #[test]
    fn test_concurrent_access() {
        let tracker = Arc::new(MetricsTracker::new());
        let mut handles = vec![];

        // Spawn 10 threads, each recording 100 events
        for i in 0..10 {
            let tracker_clone = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    tracker_clone.record_event(&format!("source{}", i));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have recorded 1000 total events
        assert_eq!(tracker.get_total_events(), 1000);

        // Should have 10 unique publishers
        assert_eq!(tracker.get_active_publisher_count(10), 10);
    }
}
