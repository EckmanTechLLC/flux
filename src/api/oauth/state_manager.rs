//! OAuth state management for CSRF protection.
//!
//! Manages temporary state tokens used to prevent CSRF attacks during OAuth flow.

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// OAuth state entry (tracks state parameter for CSRF protection)
#[derive(Clone, Debug)]
pub struct StateEntry {
    pub connector: String,
    pub namespace: String,
    pub created_at: DateTime<Utc>,
}

/// OAuth state manager with automatic expiration
#[derive(Clone)]
pub struct StateManager {
    states: Arc<Mutex<HashMap<String, StateEntry>>>,
    expiry_duration: Duration,
}

impl StateManager {
    /// Create a new state manager
    ///
    /// # Arguments
    /// * `expiry_seconds` - How long states remain valid (default: 600 = 10 minutes)
    pub fn new(expiry_seconds: i64) -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
            expiry_duration: Duration::seconds(expiry_seconds),
        }
    }

    /// Generate a new state token and store it
    ///
    /// Returns the state token (UUID v4)
    pub fn create_state(&self, connector: &str, namespace: &str) -> String {
        let state = Uuid::new_v4().to_string();
        let entry = StateEntry {
            connector: connector.to_string(),
            namespace: namespace.to_string(),
            created_at: Utc::now(),
        };

        let mut states = self.states.lock().unwrap();
        states.insert(state.clone(), entry);

        state
    }

    /// Validate and consume a state token
    ///
    /// Returns the StateEntry if valid and not expired, None otherwise.
    /// The state is removed from the map (single-use).
    pub fn validate_and_consume(&self, state: &str) -> Option<StateEntry> {
        let mut states = self.states.lock().unwrap();

        // Remove state (single-use)
        let entry = states.remove(state)?;

        // Check expiration
        let now = Utc::now();
        if now - entry.created_at > self.expiry_duration {
            return None;
        }

        Some(entry)
    }

    /// Clean up expired states (should be called periodically)
    pub fn cleanup_expired(&self) {
        let mut states = self.states.lock().unwrap();
        let now = Utc::now();

        states.retain(|_, entry| {
            now - entry.created_at <= self.expiry_duration
        });
    }

    /// Get count of active states (for debugging/monitoring)
    pub fn count(&self) -> usize {
        self.states.lock().unwrap().len()
    }
}

/// Background task to periodically clean up expired states
pub async fn run_state_cleanup(manager: StateManager, interval_seconds: u64) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));

    loop {
        interval.tick().await;
        manager.cleanup_expired();
        tracing::debug!("OAuth state cleanup complete, {} states remaining", manager.count());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_validate_state() {
        let manager = StateManager::new(600);

        let state = manager.create_state("github", "user123");
        assert!(!state.is_empty());

        let entry = manager.validate_and_consume(&state);
        assert!(entry.is_some());

        let entry = entry.unwrap();
        assert_eq!(entry.connector, "github");
        assert_eq!(entry.namespace, "user123");
    }

    #[test]
    fn test_state_is_single_use() {
        let manager = StateManager::new(600);

        let state = manager.create_state("gmail", "alice");

        // First validation succeeds
        assert!(manager.validate_and_consume(&state).is_some());

        // Second validation fails (already consumed)
        assert!(manager.validate_and_consume(&state).is_none());
    }

    #[test]
    fn test_invalid_state_rejected() {
        let manager = StateManager::new(600);

        let result = manager.validate_and_consume("invalid_state");
        assert!(result.is_none());
    }

    #[test]
    fn test_expired_state_rejected() {
        let manager = StateManager::new(1); // 1 second expiry

        let state = manager.create_state("linkedin", "bob");

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        let result = manager.validate_and_consume(&state);
        assert!(result.is_none());
    }

    #[test]
    fn test_cleanup_removes_expired() {
        let manager = StateManager::new(1); // 1 second expiry

        manager.create_state("github", "user1");
        manager.create_state("gmail", "user2");

        assert_eq!(manager.count(), 2);

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        manager.cleanup_expired();
        assert_eq!(manager.count(), 0);
    }
}
