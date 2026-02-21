use serde::{Deserialize, Serialize};

/// Identifies which runner backend handles a connector instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConnectorType {
    Builtin,
    Generic,
    Named,
}

/// OAuth configuration for a connector.
///
/// Defines the OAuth 2.0 endpoints and scopes required to authenticate
/// with the external API.
///
/// # Example
/// ```
/// use connector_manager::OAuthConfig;
///
/// let config = OAuthConfig {
///     auth_url: "https://github.com/login/oauth/authorize".to_string(),
///     token_url: "https://github.com/login/oauth/access_token".to_string(),
///     scopes: vec!["repo".to_string(), "read:user".to_string()],
/// };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth authorization endpoint URL
    pub auth_url: String,

    /// OAuth token exchange endpoint URL
    pub token_url: String,

    /// Required OAuth scopes for this connector
    pub scopes: Vec<String>,
}
