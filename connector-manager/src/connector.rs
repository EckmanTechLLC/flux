use crate::types::OAuthConfig;
use crate::Credentials;
use anyhow::Result;
use async_trait::async_trait;
use flux::FluxEvent;

/// Connector interface for external API integrations.
///
/// All connectors must implement this trait to be managed by the
/// connector manager. Connectors are stateless - all state (credentials,
/// poll schedules) is managed externally.
///
/// # Lifecycle
/// 1. Connector manager calls `oauth_config()` to get OAuth endpoints
/// 2. User authorizes via OAuth flow (managed by Flux UI)
/// 3. Manager calls `fetch(credentials)` on schedule
/// 4. Connector returns Flux events
/// 5. Manager publishes events to Flux
///
/// # Example
/// ```no_run
/// use connector_manager::{Connector, OAuthConfig, Credentials};
/// use async_trait::async_trait;
/// use anyhow::Result;
/// use flux::FluxEvent;
///
/// struct GitHubConnector;
///
/// #[async_trait]
/// impl Connector for GitHubConnector {
///     fn name(&self) -> &str {
///         "github"
///     }
///
///     fn oauth_config(&self) -> OAuthConfig {
///         OAuthConfig {
///             auth_url: "https://github.com/login/oauth/authorize".to_string(),
///             token_url: "https://github.com/login/oauth/access_token".to_string(),
///             scopes: vec!["repo".to_string()],
///         }
///     }
///
///     async fn fetch(&self, credentials: &Credentials) -> Result<Vec<FluxEvent>> {
///         // Fetch data from GitHub API using credentials
///         // Transform to Flux events
///         // Return events
///         Ok(vec![])
///     }
///
///     fn poll_interval(&self) -> u64 {
///         300 // Poll every 5 minutes
///     }
/// }
/// ```
#[async_trait]
pub trait Connector: Send + Sync {
    /// Returns the unique identifier for this connector.
    ///
    /// Must be lowercase alphanumeric (e.g., "github", "gmail").
    /// Used for API endpoints, configuration, and logging.
    fn name(&self) -> &str;

    /// Returns the OAuth configuration for this connector.
    ///
    /// Defines the OAuth endpoints and scopes required to authenticate
    /// with the external API.
    fn oauth_config(&self) -> OAuthConfig;

    /// Fetches data from the external API and returns Flux events.
    ///
    /// This is the core method where connectors implement their logic:
    /// 1. Authenticate with external API using credentials
    /// 2. Fetch data (handle pagination, rate limits)
    /// 3. Transform data into Flux events
    /// 4. Return events for publishing
    ///
    /// # Arguments
    /// * `credentials` - OAuth credentials (access token, refresh token)
    ///
    /// # Returns
    /// * `Ok(Vec<FluxEvent>)` - Events to publish to Flux
    /// * `Err(...)` - Authentication, network, or API errors
    ///
    /// # Error Handling
    /// - Auth errors (expired token) → manager will trigger re-auth
    /// - Rate limit errors → manager will back off
    /// - Network errors → manager will retry with exponential backoff
    async fn fetch(&self, credentials: &Credentials) -> Result<Vec<FluxEvent>>;

    /// Returns the poll interval in seconds.
    ///
    /// How often the connector manager should call `fetch()`.
    /// Recommended: 300 seconds (5 minutes) for most APIs.
    ///
    /// Consider API rate limits when setting this value.
    fn poll_interval(&self) -> u64;
}
