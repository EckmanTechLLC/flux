//! Per-connector polling scheduler.
//!
//! Each connector gets its own scheduler that polls on an interval,
//! fetches data, and publishes events to Flux.

use crate::{Connector, Credentials};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use flux::credentials::CredentialStore;
use flux::FluxEvent;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Token response from an OAuth token refresh endpoint.
#[derive(Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Per-connector polling scheduler.
///
/// Manages the polling lifecycle for a single connector instance:
/// - Polls on a fixed interval
/// - Refreshes OAuth tokens before expiry (90-second threshold)
/// - Fetches data from the connector
/// - Publishes events to Flux API
/// - Handles errors with exponential backoff
/// - Tracks status (last poll, errors)
pub struct ConnectorScheduler {
    /// User/namespace ID
    user_id: String,
    /// Connector implementation
    connector: Arc<dyn Connector>,
    /// OAuth credentials (updated in place on token refresh)
    credentials: Credentials,
    /// Flux API base URL (e.g., "http://localhost:3000")
    flux_api_url: String,
    /// HTTP client for publishing events and refreshing tokens
    http_client: reqwest::Client,
    /// Credential store for persisting refreshed tokens
    credential_store: Arc<CredentialStore>,
    /// Status tracking
    status: Arc<tokio::sync::Mutex<ConnectorStatus>>,
}

/// Status information for a connector instance.
#[derive(Clone, Debug)]
pub struct ConnectorStatus {
    /// Last successful poll timestamp
    pub last_poll: Option<DateTime<Utc>>,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Total number of successful polls
    pub poll_count: u64,
    /// Total number of errors
    pub error_count: u64,
}

impl Default for ConnectorStatus {
    fn default() -> Self {
        Self {
            last_poll: None,
            last_error: None,
            poll_count: 0,
            error_count: 0,
        }
    }
}

impl ConnectorScheduler {
    /// Creates a new scheduler for a connector.
    pub fn new(
        user_id: String,
        connector: Arc<dyn Connector>,
        credentials: Credentials,
        flux_api_url: String,
        credential_store: Arc<CredentialStore>,
    ) -> Self {
        Self {
            user_id,
            connector,
            credentials,
            flux_api_url,
            http_client: reqwest::Client::new(),
            credential_store,
            status: Arc::new(tokio::sync::Mutex::new(ConnectorStatus::default())),
        }
    }

    /// Returns a clone of the status tracker for external monitoring.
    pub fn status(&self) -> Arc<tokio::sync::Mutex<ConnectorStatus>> {
        Arc::clone(&self.status)
    }

    /// Returns true if the access token should be refreshed before the next poll.
    ///
    /// Refresh is triggered when `expires_at` is within 90 seconds (or already past)
    /// AND `refresh_token` is present. PAT connectors (no expiry or no refresh token)
    /// are unaffected.
    fn needs_refresh(&self) -> bool {
        match (&self.credentials.expires_at, &self.credentials.refresh_token) {
            (Some(expires_at), Some(_)) => {
                let threshold = Utc::now() + chrono::Duration::seconds(90);
                *expires_at <= threshold
            }
            _ => false,
        }
    }

    /// Attempts to refresh the OAuth access token.
    ///
    /// POSTs to the connector's token endpoint with `grant_type=refresh_token`.
    /// Client credentials are included if `FLUX_OAUTH_{CONNECTOR}_CLIENT_ID` /
    /// `FLUX_OAUTH_{CONNECTOR}_CLIENT_SECRET` are set in the environment.
    ///
    /// On success, updates credentials in memory and persists to the credential store.
    /// On failure, returns an error — the caller skips the poll.
    async fn try_refresh_token(&mut self) -> Result<()> {
        let refresh_token = match &self.credentials.refresh_token {
            Some(t) => t.clone(),
            None => return Ok(()),
        };

        let oauth_config = self.connector.oauth_config();
        let connector_name = self.connector.name().to_string();
        let env_prefix = connector_name.to_uppercase();

        let mut form: HashMap<String, String> = HashMap::new();
        form.insert("grant_type".to_string(), "refresh_token".to_string());
        form.insert("refresh_token".to_string(), refresh_token);

        // Include client credentials if configured in the environment
        if let Ok(client_id) = std::env::var(format!("FLUX_OAUTH_{}_CLIENT_ID", env_prefix)) {
            form.insert("client_id".to_string(), client_id);
        }
        if let Ok(client_secret) =
            std::env::var(format!("FLUX_OAUTH_{}_CLIENT_SECRET", env_prefix))
        {
            form.insert("client_secret".to_string(), client_secret);
        }

        info!(
            user_id = %self.user_id,
            connector = %connector_name,
            "Refreshing OAuth token"
        );

        let response = self
            .http_client
            .post(&oauth_config.token_url)
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .context("Failed to send token refresh request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            anyhow::bail!("Token refresh failed with status {}: {}", status, body);
        }

        let token_response: TokenRefreshResponse = response
            .json()
            .await
            .context("Failed to parse token refresh response")?;

        let expires_at = token_response
            .expires_in
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

        // Keep the existing refresh token if the provider did not rotate it
        let new_refresh_token = token_response
            .refresh_token
            .or_else(|| self.credentials.refresh_token.clone());

        let new_credentials = Credentials {
            access_token: token_response.access_token,
            refresh_token: new_refresh_token,
            expires_at,
        };

        self.credential_store
            .store(&self.user_id, &connector_name, &new_credentials)
            .context("Failed to persist refreshed credentials")?;

        self.credentials = new_credentials;

        info!(
            user_id = %self.user_id,
            connector = %connector_name,
            "OAuth token refreshed successfully"
        );

        Ok(())
    }

    /// Starts the polling loop (non-blocking).
    ///
    /// Spawns a background task that polls the connector on schedule.
    /// Returns a JoinHandle that can be used for graceful shutdown.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        let poll_interval_secs = self.connector.poll_interval();
        let connector_name = self.connector.name().to_string();
        let user_id = self.user_id.clone();

        tokio::spawn(async move {
            info!(
                user_id = %user_id,
                connector = %connector_name,
                interval_secs = poll_interval_secs,
                "Starting connector scheduler"
            );

            let mut interval = interval(Duration::from_secs(poll_interval_secs));
            let mut scheduler = self;

            loop {
                interval.tick().await;

                debug!(
                    user_id = %user_id,
                    connector = %connector_name,
                    "Polling connector"
                );

                // Refresh token if within 90 seconds of expiry before polling
                if scheduler.needs_refresh() {
                    if let Err(e) = scheduler.try_refresh_token().await {
                        error!(
                            user_id = %user_id,
                            connector = %connector_name,
                            error = %e,
                            "Token refresh failed, skipping poll"
                        );
                        let mut status = scheduler.status.lock().await;
                        status.last_error = Some(format!("Token refresh failed: {}", e));
                        status.error_count += 1;
                        continue;
                    }
                }

                if let Err(e) = scheduler.fetch_and_publish_with_retry().await {
                    error!(
                        user_id = %user_id,
                        connector = %connector_name,
                        error = %e,
                        "Failed to fetch and publish events after retries"
                    );

                    // Update status with error
                    let mut status = scheduler.status.lock().await;
                    status.last_error = Some(e.to_string());
                    status.error_count += 1;
                } else {
                    // Update status on success
                    let mut status = scheduler.status.lock().await;
                    status.last_poll = Some(Utc::now());
                    status.last_error = None;
                    status.poll_count += 1;
                }
            }
        })
    }

    /// Fetches data and publishes to Flux with retry logic.
    async fn fetch_and_publish_with_retry(&self) -> Result<()> {
        const MAX_RETRIES: u32 = 3;
        const BACKOFF_DELAYS: [u64; 3] = [60, 120, 240]; // seconds

        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match self.fetch_and_publish().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!(
                        user_id = %self.user_id,
                        connector = %self.connector.name(),
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        error = %e,
                        "Fetch and publish failed, will retry"
                    );

                    last_error = Some(e);

                    if attempt < MAX_RETRIES - 1 {
                        let delay_secs = BACKOFF_DELAYS[attempt as usize];
                        debug!(
                            user_id = %self.user_id,
                            connector = %self.connector.name(),
                            delay_secs = delay_secs,
                            "Backing off before retry"
                        );
                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Fetches data from connector and publishes to Flux.
    async fn fetch_and_publish(&self) -> Result<()> {
        // 1. Fetch events from connector
        let events = self
            .connector
            .fetch(&self.credentials)
            .await
            .context("Failed to fetch data from connector")?;

        if events.is_empty() {
            debug!(
                user_id = %self.user_id,
                connector = %self.connector.name(),
                "No events to publish"
            );
            return Ok(());
        }

        info!(
            user_id = %self.user_id,
            connector = %self.connector.name(),
            event_count = events.len(),
            "Fetched events from connector"
        );

        // 2. Publish events to Flux API
        self.publish_events(&events).await?;

        Ok(())
    }

    /// Publishes events to Flux API via HTTP POST.
    async fn publish_events(&self, events: &[FluxEvent]) -> Result<()> {
        let url = format!("{}/api/events", self.flux_api_url);

        for event in events {
            let response = self
                .http_client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", self.user_id))
                .json(event)
                .send()
                .await
                .context("Failed to send HTTP request to Flux API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<failed to read body>".to_string());

                anyhow::bail!(
                    "Flux API returned error status {}: {}",
                    status,
                    body
                );
            }
        }

        info!(
            user_id = %self.user_id,
            connector = %self.connector.name(),
            event_count = events.len(),
            "Published events to Flux API"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::github::GitHubConnector;
    use crate::{Connector, OAuthConfig};
    use async_trait::async_trait;

    fn make_store() -> Arc<CredentialStore> {
        let key = base64::encode(&[0u8; 32]);
        Arc::new(CredentialStore::new(":memory:", &key).expect("Failed to create test store"))
    }

    fn make_scheduler(credentials: Credentials) -> ConnectorScheduler {
        ConnectorScheduler::new(
            "test_user".to_string(),
            Arc::new(GitHubConnector::new()),
            credentials,
            "http://localhost:3000".to_string(),
            make_store(),
        )
    }

    // --- needs_refresh ---

    #[test]
    fn test_needs_refresh_no_refresh_token() {
        let s = make_scheduler(Credentials {
            access_token: "tok".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::seconds(30)),
        });
        assert!(!s.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_no_expiry() {
        let s = make_scheduler(Credentials {
            access_token: "tok".to_string(),
            refresh_token: Some("r".to_string()),
            expires_at: None,
        });
        assert!(!s.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_far_future() {
        let s = make_scheduler(Credentials {
            access_token: "tok".to_string(),
            refresh_token: Some("r".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::hours(2)),
        });
        assert!(!s.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_near_expiry() {
        let s = make_scheduler(Credentials {
            access_token: "tok".to_string(),
            refresh_token: Some("r".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::seconds(30)),
        });
        assert!(s.needs_refresh());
    }

    #[test]
    fn test_needs_refresh_already_expired() {
        let s = make_scheduler(Credentials {
            access_token: "tok".to_string(),
            refresh_token: Some("r".to_string()),
            expires_at: Some(Utc::now() - chrono::Duration::seconds(1)),
        });
        assert!(s.needs_refresh());
    }

    // --- try_refresh_token ---

    /// Test connector whose token_url can be pointed at a mock server.
    struct MockConnector {
        token_url: String,
    }

    #[async_trait]
    impl Connector for MockConnector {
        fn name(&self) -> &str {
            "mockconn"
        }
        fn oauth_config(&self) -> OAuthConfig {
            OAuthConfig {
                auth_url: "https://example.com/auth".to_string(),
                token_url: self.token_url.clone(),
                scopes: vec![],
            }
        }
        async fn fetch(&self, _: &Credentials) -> anyhow::Result<Vec<FluxEvent>> {
            Ok(vec![])
        }
        fn poll_interval(&self) -> u64 {
            300
        }
    }

    #[tokio::test]
    async fn test_try_refresh_token_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"new_token","expires_in":3600}"#)
            .create_async()
            .await;

        let store = make_store();
        let connector = Arc::new(MockConnector {
            token_url: format!("{}/token", server.url()),
        });

        let mut scheduler = ConnectorScheduler::new(
            "test_user".to_string(),
            connector,
            Credentials {
                access_token: "old_token".to_string(),
                refresh_token: Some("my_refresh".to_string()),
                expires_at: Some(Utc::now() + chrono::Duration::seconds(30)),
            },
            "http://localhost:3000".to_string(),
            Arc::clone(&store),
        );

        let result = scheduler.try_refresh_token().await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(scheduler.credentials.access_token, "new_token");
        // Provider did not rotate — original refresh token must be kept
        assert_eq!(
            scheduler.credentials.refresh_token,
            Some("my_refresh".to_string())
        );

        // Verify credentials were persisted to the store
        let stored = store.get("test_user", "mockconn").unwrap().unwrap();
        assert_eq!(stored.access_token, "new_token");

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_try_refresh_token_http_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/token")
            .with_status(400)
            .with_body(r#"{"error":"invalid_grant"}"#)
            .create_async()
            .await;

        let store = make_store();
        let connector = Arc::new(MockConnector {
            token_url: format!("{}/token", server.url()),
        });

        let mut scheduler = ConnectorScheduler::new(
            "test_user".to_string(),
            connector,
            Credentials {
                access_token: "old_token".to_string(),
                refresh_token: Some("expired_refresh".to_string()),
                expires_at: Some(Utc::now() + chrono::Duration::seconds(30)),
            },
            "http://localhost:3000".to_string(),
            Arc::clone(&store),
        );

        let result = scheduler.try_refresh_token().await;
        assert!(result.is_err(), "Expected Err on 400 response");
        // Credentials must be unchanged after failed refresh
        assert_eq!(scheduler.credentials.access_token, "old_token");

        mock.assert_async().await;
    }

    // --- existing tests (updated for new constructor signature) ---

    #[tokio::test]
    async fn test_scheduler_status() {
        let scheduler = make_scheduler(Credentials {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_at: None,
        });

        let status = scheduler.status();
        let status_data = status.lock().await;
        assert_eq!(status_data.poll_count, 0);
        assert_eq!(status_data.error_count, 0);
        assert!(status_data.last_poll.is_none());
    }

    #[tokio::test]
    async fn test_fetch_and_publish_no_server() {
        // This test verifies error handling when Flux API is unreachable
        let connector = Arc::new(GitHubConnector::new());
        let scheduler = ConnectorScheduler::new(
            "test_user".to_string(),
            connector,
            Credentials {
                access_token: "test_token".to_string(),
                refresh_token: None,
                expires_at: None,
            },
            "http://localhost:9999".to_string(), // Invalid port
            make_store(),
        );

        let result = scheduler.fetch_and_publish().await;
        assert!(result.is_err());
    }
}
