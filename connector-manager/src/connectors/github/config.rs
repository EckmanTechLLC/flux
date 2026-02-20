use crate::OAuthConfig;
use anyhow::{Context, Result};

pub const BASE_URL: &str = "https://api.github.com";
pub const AUTH_URL: &str = "https://github.com/login/oauth/authorize";
pub const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const SCOPES: &[&str] = &["repo", "read:user", "notifications"];

/// GitHub OAuth configuration.
///
/// Loads client ID and secret from environment variables:
/// - `FLUX_OAUTH_GITHUB_CLIENT_ID`
/// - `FLUX_OAUTH_GITHUB_CLIENT_SECRET`
#[derive(Debug)]
pub struct GitHubConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl GitHubConfig {
    /// Load config from environment variables.
    pub fn from_env() -> Result<Self> {
        let client_id = std::env::var("FLUX_OAUTH_GITHUB_CLIENT_ID")
            .context("FLUX_OAUTH_GITHUB_CLIENT_ID not set")?;
        let client_secret = std::env::var("FLUX_OAUTH_GITHUB_CLIENT_SECRET")
            .context("FLUX_OAUTH_GITHUB_CLIENT_SECRET not set")?;
        Ok(Self {
            client_id,
            client_secret,
        })
    }

    /// Returns the OAuthConfig for GitHub.
    pub fn oauth_config(&self) -> OAuthConfig {
        OAuthConfig {
            auth_url: AUTH_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            scopes: SCOPES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize all env-var-mutating tests to avoid race conditions between
    // tests that run concurrently but share the process-wide env.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_constants() {
        assert_eq!(BASE_URL, "https://api.github.com");
        assert_eq!(AUTH_URL, "https://github.com/login/oauth/authorize");
        assert_eq!(TOKEN_URL, "https://github.com/login/oauth/access_token");
        assert_eq!(SCOPES, &["repo", "read:user", "notifications"]);
    }

    #[test]
    fn test_from_env_missing() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_ID");
        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_SECRET");

        let result = GitHubConfig::from_env();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("FLUX_OAUTH_GITHUB_CLIENT_ID"));
    }

    #[test]
    fn test_from_env_success() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("FLUX_OAUTH_GITHUB_CLIENT_ID", "test_client_id");
        std::env::set_var("FLUX_OAUTH_GITHUB_CLIENT_SECRET", "test_client_secret");

        let config = GitHubConfig::from_env().unwrap();
        assert_eq!(config.client_id, "test_client_id");
        assert_eq!(config.client_secret, "test_client_secret");

        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_ID");
        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_SECRET");
    }

    #[test]
    fn test_oauth_config() {
        let _lock = ENV_LOCK.lock().unwrap();
        std::env::set_var("FLUX_OAUTH_GITHUB_CLIENT_ID", "id");
        std::env::set_var("FLUX_OAUTH_GITHUB_CLIENT_SECRET", "secret");

        let config = GitHubConfig::from_env().unwrap();
        let oauth = config.oauth_config();

        assert_eq!(oauth.auth_url, AUTH_URL);
        assert_eq!(oauth.token_url, TOKEN_URL);
        assert_eq!(oauth.scopes, vec!["repo", "read:user", "notifications"]);

        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_ID");
        std::env::remove_var("FLUX_OAUTH_GITHUB_CLIENT_SECRET");
    }
}
