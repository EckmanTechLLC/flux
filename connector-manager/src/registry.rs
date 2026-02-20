//! Connector registry - Manages available connectors.
//!
//! Phase 1: Hardcoded mock connectors for testing.
//! Phase 2+: Dynamic connector loading (plugins, WASM).

use crate::connectors::github::GitHubConnector;
use crate::Connector;
use std::sync::Arc;

/// Returns all available connectors.
pub fn get_all_connectors() -> Vec<Arc<dyn Connector>> {
    vec![Arc::new(GitHubConnector::new())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_connector() {
        let connector = GitHubConnector::new();
        assert_eq!(connector.name(), "github");
        assert_eq!(connector.poll_interval(), 300);

        let oauth_config = connector.oauth_config();
        assert!(oauth_config.auth_url.contains("github.com"));
        assert_eq!(oauth_config.scopes.len(), 3);
    }

    #[test]
    fn test_get_all_connectors() {
        let connectors = get_all_connectors();
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].name(), "github");
    }
}
