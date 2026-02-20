//! OAuth provider configurations.
//!
//! Defines OAuth 2.0 configuration for each supported external service.

use serde::{Deserialize, Serialize};

/// OAuth provider configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthProviderConfig {
    /// OAuth authorization endpoint URL
    pub auth_url: String,

    /// OAuth token exchange endpoint URL
    pub token_url: String,

    /// Required OAuth scopes
    pub scopes: Vec<String>,

    /// Client ID (from environment variable)
    pub client_id: String,

    /// Client secret (from environment variable)
    pub client_secret: String,
}

impl OAuthProviderConfig {
    /// Build authorization URL with state and redirect_uri
    pub fn build_auth_url(&self, state: &str, redirect_uri: &str) -> String {
        let scopes = self.scopes.join(" ");
        format!(
            "{}?client_id={}&redirect_uri={}&scope={}&state={}&response_type=code",
            self.auth_url,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state)
        )
    }
}

/// Get OAuth provider configuration by connector name
pub fn get_provider_config(connector_name: &str) -> Option<OAuthProviderConfig> {
    // Load client ID and secret from environment
    let env_prefix = connector_name.to_uppercase();
    let client_id = std::env::var(format!("FLUX_OAUTH_{}_CLIENT_ID", env_prefix)).ok()?;
    let client_secret = std::env::var(format!("FLUX_OAUTH_{}_CLIENT_SECRET", env_prefix)).ok()?;

    let (auth_url, token_url, scopes) = match connector_name {
        "github" => (
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
            vec!["repo", "read:user"],
        ),
        "gmail" => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
            vec!["https://www.googleapis.com/auth/gmail.readonly"],
        ),
        "linkedin" => (
            "https://www.linkedin.com/oauth/v2/authorization",
            "https://www.linkedin.com/oauth/v2/accessToken",
            vec!["r_liteprofile", "r_emailaddress"],
        ),
        "calendar" => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
            vec!["https://www.googleapis.com/auth/calendar.readonly"],
        ),
        _ => return None,
    };

    Some(OAuthProviderConfig {
        auth_url: auth_url.to_string(),
        token_url: token_url.to_string(),
        scopes: scopes.into_iter().map(|s| s.to_string()).collect(),
        client_id,
        client_secret,
    })
}

/// Check if a connector name is valid
pub fn is_valid_connector(name: &str) -> bool {
    matches!(name, "github" | "gmail" | "linkedin" | "calendar")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_connector_names() {
        assert!(is_valid_connector("github"));
        assert!(is_valid_connector("gmail"));
        assert!(is_valid_connector("linkedin"));
        assert!(is_valid_connector("calendar"));
        assert!(!is_valid_connector("invalid"));
        assert!(!is_valid_connector(""));
    }

    #[test]
    fn test_build_auth_url() {
        let config = OAuthProviderConfig {
            auth_url: "https://example.com/oauth/authorize".to_string(),
            token_url: "https://example.com/oauth/token".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            client_id: "test_client_id".to_string(),
            client_secret: "test_secret".to_string(),
        };

        let url = config.build_auth_url("random_state", "http://localhost:3000/callback");

        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fcallback"));
        // URL encoding converts spaces to %20
        assert!(url.contains("scope=read%20write"));
        assert!(url.contains("state=random_state"));
        assert!(url.contains("response_type=code"));
    }
}
