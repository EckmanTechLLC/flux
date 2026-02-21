//! Connector manager - Orchestrates connector lifecycle.
//!
//! Loads available connectors, retrieves credentials from storage,
//! and starts polling schedulers for each user-connector pair.

use crate::registry::get_all_connectors;
use crate::runners::builtin::{ConnectorScheduler, ConnectorStatus};
use anyhow::{Context, Result};
use flux::credentials::CredentialStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time;
use tracing::{info, warn};

/// Connector manager - Orchestrates all connector polling.
///
/// # Responsibilities
/// - Load available connectors
/// - Fetch credentials for each user-connector pair
/// - Start scheduler for each active connector
/// - Track status for all connectors
/// - Graceful shutdown
pub struct ConnectorManager {
    /// Credential store (for fetching OAuth tokens)
    credential_store: Arc<CredentialStore>,
    /// Flux API base URL
    flux_api_url: String,
    /// Discovery loop task handle
    scheduler_handles: Vec<JoinHandle<()>>,
    /// Status tracking per (user_id, connector) pair
    status_map: Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<ConnectorStatus>>>>>,
    /// Per-key scheduler handles — enables per-key abort/restart
    connector_handles: Arc<tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl ConnectorManager {
    /// Creates a new connector manager.
    ///
    /// # Arguments
    /// * `credential_store` - Store for retrieving OAuth credentials
    /// * `flux_api_url` - Base URL for Flux API (e.g., "http://localhost:3000")
    pub fn new(credential_store: Arc<CredentialStore>, flux_api_url: String) -> Self {
        Self {
            credential_store,
            flux_api_url,
            scheduler_handles: Vec::new(),
            status_map: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            connector_handles: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Returns a clone of the status map for external monitoring.
    pub fn status_map(
        &self,
    ) -> Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<ConnectorStatus>>>>> {
        Arc::clone(&self.status_map)
    }

    /// Starts the connector manager.
    ///
    /// Loads all available connectors and starts polling for each user that has credentials.
    ///
    /// # Phase 1 Approach
    /// Since we don't have a user table yet, we'll scan the credential store
    /// for all unique (user_id, connector) pairs and start schedulers for each.
    ///
    /// # Returns
    /// Number of schedulers started
    pub async fn start(&mut self) -> Result<usize> {
        info!("Starting connector manager");

        // Load all available connectors
        let connectors = get_all_connectors();
        info!(connector_count = connectors.len(), "Loaded connectors");

        if connectors.is_empty() {
            warn!("No connectors available, nothing to start");
            return Ok(0);
        }

        // Enumerate all stored credentials and start a scheduler for each
        let all_credentials = self
            .credential_store
            .list_all()
            .context("Failed to enumerate credentials")?;

        let mut started_count = 0;
        for (user_id, connector_name) in &all_credentials {
            if !connectors.iter().any(|c| c.name() == connector_name.as_str()) {
                warn!(connector = %connector_name, "Skipping unknown connector in credential store");
                continue;
            }
            match self.start_connector_for_user(user_id, connector_name).await {
                Ok(()) => started_count += 1,
                Err(e) => warn!(
                    user_id = %user_id,
                    connector = %connector_name,
                    error = %e,
                    "Failed to start scheduler"
                ),
            }
        }

        if started_count == 0 {
            info!("No credentials found - waiting for OAuth authorization");
        }

        // Spawn background task to run discovery every 60s:
        //  - picks up newly stored credentials
        //  - restarts schedulers that have entered an error state
        //  - removes schedulers whose credentials were deleted
        let cred_store = Arc::clone(&self.credential_store);
        let status_map = Arc::clone(&self.status_map);
        let conn_handles = Arc::clone(&self.connector_handles);
        let flux_url = self.flux_api_url.clone();

        let discovery_handle = tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(60));
            interval.tick().await; // consume immediate first tick — initial scan done above

            loop {
                interval.tick().await;
                run_discovery_cycle(&cred_store, &status_map, &conn_handles, &flux_url).await;
            }
        });

        self.scheduler_handles.push(discovery_handle);

        Ok(started_count)
    }

    /// Starts a scheduler for a specific user-connector pair.
    ///
    /// Aborts any existing scheduler for the same key before starting a new one.
    ///
    /// # Arguments
    /// * `user_id` - User/namespace ID
    /// * `connector_name` - Connector identifier (e.g., "github")
    pub async fn start_connector_for_user(
        &mut self,
        user_id: &str,
        connector_name: &str,
    ) -> Result<()> {
        info!(
            user_id = %user_id,
            connector = %connector_name,
            "Starting connector scheduler"
        );

        // Find the connector
        let connectors = get_all_connectors();
        let connector = connectors
            .iter()
            .find(|c| c.name() == connector_name)
            .context(format!("Connector '{}' not found", connector_name))?;

        // Get credentials
        let credentials = self
            .credential_store
            .get(user_id, connector_name)?
            .context(format!(
                "No credentials found for user '{}' connector '{}'",
                user_id, connector_name
            ))?;

        info!(
            user_id = %user_id,
            connector = %connector_name,
            "Retrieved credentials"
        );

        // Create scheduler
        let scheduler = ConnectorScheduler::new(
            user_id.to_string(),
            Arc::clone(connector),
            credentials,
            self.flux_api_url.clone(),
            Arc::clone(&self.credential_store),
        );

        let status_handle = scheduler.status();
        let handle = scheduler.start();

        let status_key = format!("{}:{}", user_id, connector_name);

        // Abort existing scheduler for this key if any, then track new handle
        {
            let mut handles = self.connector_handles.lock().await;
            if let Some(old) = handles.remove(&status_key) {
                old.abort();
                info!(key = %status_key, "Aborted existing scheduler before restart");
            }
            handles.insert(status_key.clone(), handle);
        }

        self.status_map.lock().await.insert(status_key, status_handle);

        info!(
            user_id = %user_id,
            connector = %connector_name,
            "Connector scheduler started"
        );

        Ok(())
    }

    /// Shuts down all connector schedulers gracefully.
    ///
    /// Aborts all running tasks and waits for them to complete.
    pub async fn shutdown(&mut self) {
        info!("Shutting down connector manager");

        // Abort discovery loop
        for handle in self.scheduler_handles.drain(..) {
            handle.abort();
        }

        // Abort all per-connector schedulers
        let mut handles = self.connector_handles.lock().await;
        let count = handles.len();
        if count > 0 {
            info!(scheduler_count = count, "Aborting connector scheduler tasks");
            for (_, handle) in handles.drain() {
                handle.abort();
            }
        }

        info!("All scheduler tasks aborted");
    }
}

impl Drop for ConnectorManager {
    fn drop(&mut self) {
        // Abort discovery loop (non-async)
        for handle in self.scheduler_handles.drain(..) {
            handle.abort();
        }
        // Best-effort abort of per-connector schedulers (try_lock since Drop is sync)
        if let Ok(mut handles) = self.connector_handles.try_lock() {
            for (_, handle) in handles.drain() {
                handle.abort();
            }
        }
    }
}

/// Runs one iteration of the credential discovery cycle.
///
/// Three responsibilities:
/// 1. Remove schedulers for credentials that have been deleted
/// 2. Restart schedulers that have entered an error state (fresh credentials)
/// 3. Start schedulers for newly added credentials
async fn run_discovery_cycle(
    cred_store: &Arc<CredentialStore>,
    status_map: &Arc<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<ConnectorStatus>>>>>,
    connector_handles: &Arc<tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>>,
    flux_url: &str,
) {
    let all_creds = match cred_store.list_all() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Credential discovery: failed to list credentials");
            return;
        }
    };

    let connectors = get_all_connectors();

    // Build set of currently-credentialed keys for O(1) lookup
    let cred_keys: std::collections::HashSet<String> = all_creds
        .iter()
        .map(|(uid, cname)| format!("{}:{}", uid, cname))
        .collect();

    // Snapshot existing entries without holding the map lock during status reads
    let existing: Vec<(String, Arc<tokio::sync::Mutex<ConnectorStatus>>)> = {
        let map = status_map.lock().await;
        map.iter().map(|(k, v)| (k.clone(), Arc::clone(v))).collect()
    };

    let mut to_remove: Vec<String> = Vec::new();
    let mut to_restart: Vec<String> = Vec::new();

    for (key, status_arc) in &existing {
        if !cred_keys.contains(key) {
            to_remove.push(key.clone());
        } else {
            let status = status_arc.lock().await;
            if status.last_error.is_some() {
                to_restart.push(key.clone());
            }
        }
    }

    // 1. Abort schedulers for deleted credentials
    for key in &to_remove {
        {
            let mut handles = connector_handles.lock().await;
            if let Some(handle) = handles.remove(key) {
                handle.abort();
            }
        }
        status_map.lock().await.remove(key);
        info!(key = %key, "Discovery: removed scheduler (credentials deleted)");
    }

    // 2. Restart schedulers in error state
    for key in &to_restart {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            warn!(key = %key, "Discovery: skipping invalid key format");
            continue;
        }
        let (user_id, connector_name) = (parts[0], parts[1]);

        // Abort old handle
        {
            let mut handles = connector_handles.lock().await;
            if let Some(old) = handles.remove(key) {
                old.abort();
            }
        }

        // Fetch fresh credentials
        let credentials = match cred_store.get(user_id, connector_name) {
            Ok(Some(c)) => c,
            Ok(None) => {
                warn!(key = %key, "Discovery: credentials disappeared during restart");
                status_map.lock().await.remove(key);
                continue;
            }
            Err(e) => {
                warn!(key = %key, error = %e, "Discovery: failed to get credentials for restart");
                continue;
            }
        };

        let connector = match connectors.iter().find(|c| c.name() == connector_name) {
            Some(c) => Arc::clone(c),
            None => {
                warn!(key = %key, "Discovery: connector not found for restart");
                continue;
            }
        };

        let scheduler = ConnectorScheduler::new(
            user_id.to_string(),
            connector,
            credentials,
            flux_url.to_string(),
            Arc::clone(cred_store),
        );

        let new_status = scheduler.status();
        let new_handle = scheduler.start();

        connector_handles.lock().await.insert(key.clone(), new_handle);
        status_map.lock().await.insert(key.clone(), new_status);

        info!(key = %key, "Discovery: restarted errored scheduler");
    }

    // 3. Start schedulers for newly added credentials
    let new_pairs: Vec<(String, String)> = {
        let map = status_map.lock().await;
        all_creds
            .into_iter()
            .filter(|(uid, cname)| !map.contains_key(&format!("{}:{}", uid, cname)))
            .filter(|(_, cname)| connectors.iter().any(|c| c.name() == cname.as_str()))
            .collect()
    };

    if new_pairs.is_empty() {
        return;
    }

    info!(
        count = new_pairs.len(),
        "Discovery: starting schedulers for new credentials"
    );

    for (user_id, connector_name) in &new_pairs {
        let credentials = match cred_store.get(user_id, connector_name) {
            Ok(Some(c)) => c,
            Ok(None) => {
                warn!(
                    user_id = %user_id,
                    connector = %connector_name,
                    "Discovery: credentials disappeared"
                );
                continue;
            }
            Err(e) => {
                warn!(
                    user_id = %user_id,
                    connector = %connector_name,
                    error = %e,
                    "Discovery: failed to get credentials"
                );
                continue;
            }
        };

        let connector = match connectors.iter().find(|c| c.name() == connector_name.as_str()) {
            Some(c) => Arc::clone(c),
            None => continue,
        };

        let scheduler = ConnectorScheduler::new(
            user_id.clone(),
            connector,
            credentials,
            flux_url.to_string(),
            Arc::clone(cred_store),
        );

        let status_handle = scheduler.status();
        let handle = scheduler.start();

        let key = format!("{}:{}", user_id, connector_name);
        status_map.lock().await.insert(key.clone(), status_handle);
        connector_handles.lock().await.insert(key, handle);

        info!(
            user_id = %user_id,
            connector = %connector_name,
            "Discovery: started scheduler"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use flux::credentials::Credentials;

    #[tokio::test]
    async fn test_manager_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();
        let store = Arc::new(store);

        let manager = ConnectorManager::new(store, "http://localhost:3000".to_string());
        assert_eq!(manager.scheduler_handles.len(), 0);
    }

    #[tokio::test]
    async fn test_start_connector_for_user() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();

        // Store test credentials
        let credentials = Credentials {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        };
        store.store("test_user", "github", &credentials).unwrap();

        let store = Arc::new(store);
        let mut manager = ConnectorManager::new(store, "http://localhost:3000".to_string());

        // Start connector for user
        let result = manager.start_connector_for_user("test_user", "github").await;
        assert!(result.is_ok());

        // Handle stored in connector_handles, not scheduler_handles
        {
            let handles = manager.connector_handles.lock().await;
            assert_eq!(handles.len(), 1);
        }

        // Verify status tracking
        {
            let status_map = manager.status_map.lock().await;
            assert!(status_map.contains_key("test_user:github"));
        }

        // Cleanup
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn test_start_connector_missing_credentials() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();
        let store = Arc::new(store);

        let mut manager = ConnectorManager::new(store, "http://localhost:3000".to_string());

        // Try to start connector without credentials
        let result = manager.start_connector_for_user("test_user", "github").await;
        assert!(result.is_err());
        assert_eq!(manager.scheduler_handles.len(), 0);
    }

    #[tokio::test]
    async fn test_shutdown() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();

        let credentials = Credentials {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        store.store("test_user", "github", &credentials).unwrap();

        let store = Arc::new(store);
        let mut manager = ConnectorManager::new(store, "http://localhost:3000".to_string());

        manager
            .start_connector_for_user("test_user", "github")
            .await
            .unwrap();

        {
            let handles = manager.connector_handles.lock().await;
            assert_eq!(handles.len(), 1);
        }

        manager.shutdown().await;

        {
            let handles = manager.connector_handles.lock().await;
            assert_eq!(handles.len(), 0);
        }
    }

    /// Verifies that a scheduler whose status shows an error is aborted and
    /// restarted with fresh credentials on the next discovery cycle.
    #[tokio::test]
    async fn test_discovery_restarts_errored_scheduler() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();
        let credentials = Credentials {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        store.store("test_user", "github", &credentials).unwrap();
        let store = Arc::new(store);

        let status_map: Arc<
            tokio::sync::Mutex<
                HashMap<String, Arc<tokio::sync::Mutex<ConnectorStatus>>>,
            >,
        > = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let connector_handles: Arc<tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        // Simulate an existing scheduler that has entered an error state
        let errored_status = Arc::new(tokio::sync::Mutex::new(ConnectorStatus {
            last_error: Some("auth failed".to_string()),
            last_poll: None,
            poll_count: 0,
            error_count: 1,
        }));
        let dummy_handle: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        });

        status_map
            .lock()
            .await
            .insert("test_user:github".to_string(), Arc::clone(&errored_status));
        connector_handles
            .lock()
            .await
            .insert("test_user:github".to_string(), dummy_handle);

        // Run one discovery cycle
        run_discovery_cycle(&store, &status_map, &connector_handles, "http://localhost:3000")
            .await;

        // Verify: entry still exists but the status Arc was replaced
        let map = status_map.lock().await;
        assert!(
            map.contains_key("test_user:github"),
            "entry should still exist after restart"
        );
        let new_status_arc = map.get("test_user:github").unwrap();
        assert!(
            !Arc::ptr_eq(new_status_arc, &errored_status),
            "status Arc should have been replaced by a fresh one"
        );

        let new_status = new_status_arc.lock().await;
        assert!(
            new_status.last_error.is_none(),
            "restarted scheduler should start with no error"
        );
    }

    /// Verifies that a scheduler is aborted and removed from status_map when
    /// its credentials are deleted from the credential store.
    #[tokio::test]
    async fn test_discovery_removes_deleted_credentials() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let encryption_key = base64::encode(&[0u8; 32]);

        let store = CredentialStore::new(db_path.to_str().unwrap(), &encryption_key).unwrap();
        // No credentials stored — simulates credential deletion
        let store = Arc::new(store);

        let status_map: Arc<
            tokio::sync::Mutex<
                HashMap<String, Arc<tokio::sync::Mutex<ConnectorStatus>>>,
            >,
        > = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let connector_handles: Arc<tokio::sync::Mutex<HashMap<String, JoinHandle<()>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        // Simulate a running scheduler whose credentials have since been deleted
        let stale_status = Arc::new(tokio::sync::Mutex::new(ConnectorStatus::default()));
        let dummy_handle: JoinHandle<()> = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        });

        status_map
            .lock()
            .await
            .insert("test_user:github".to_string(), stale_status);
        connector_handles
            .lock()
            .await
            .insert("test_user:github".to_string(), dummy_handle);

        // Run one discovery cycle
        run_discovery_cycle(&store, &status_map, &connector_handles, "http://localhost:3000")
            .await;

        // Verify: entry removed from both maps
        let map = status_map.lock().await;
        assert!(
            !map.contains_key("test_user:github"),
            "deleted credentials should remove the entry from status_map"
        );

        let handles = connector_handles.lock().await;
        assert!(
            !handles.contains_key("test_user:github"),
            "deleted credentials should remove the handle from connector_handles"
        );
    }
}
