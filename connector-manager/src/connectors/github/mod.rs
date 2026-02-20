pub mod api;
pub mod config;
pub mod transformer;

use crate::{Connector, Credentials, OAuthConfig};
use anyhow::Result;
use async_trait::async_trait;
use flux::FluxEvent;

use self::api::GitHubClient;
use self::config::{AUTH_URL, BASE_URL, SCOPES, TOKEN_URL};
use self::transformer::{issue_to_event, notification_to_event, repo_to_event};

/// GitHub connector — polls the GitHub REST API and emits Flux events
/// for repositories, notifications, and open issues.
pub struct GitHubConnector {
    base_url: String,
}

impl GitHubConnector {
    /// Create a connector using the real GitHub API base URL.
    pub fn new() -> Self {
        Self {
            base_url: BASE_URL.to_string(),
        }
    }

    /// Create a connector with a custom API base URL (for testing).
    pub fn with_base_url(base_url: String) -> Self {
        Self { base_url }
    }
}

#[async_trait]
impl Connector for GitHubConnector {
    fn name(&self) -> &str {
        "github"
    }

    fn oauth_config(&self) -> OAuthConfig {
        OAuthConfig {
            auth_url: AUTH_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            scopes: SCOPES.iter().map(|s| s.to_string()).collect(),
        }
    }

    async fn fetch(&self, credentials: &Credentials) -> Result<Vec<FluxEvent>> {
        let client =
            GitHubClient::with_base_url(credentials.access_token.clone(), self.base_url.clone());
        let mut events = Vec::new();

        // Fetch repos; for each repo also fetch its open issues.
        let repos = client.fetch_repos().await?;
        for repo in &repos {
            events.push(repo_to_event(repo));
            if let Some((owner, name)) = repo.full_name.split_once('/') {
                match client.fetch_issues(owner, name).await {
                    Ok(issues) => {
                        for issue in &issues {
                            events.push(issue_to_event(owner, name, issue));
                        }
                    }
                    Err(e) => {
                        // Non-fatal: log and continue with remaining repos.
                        tracing::warn!("Failed to fetch issues for {}: {}", repo.full_name, e);
                    }
                }
            }
        }

        // Fetch notifications.
        let notifications = client.fetch_notifications().await?;
        for notification in &notifications {
            events.push(notification_to_event(notification));
        }

        Ok(events)
    }

    fn poll_interval(&self) -> u64 {
        300 // 5 minutes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Credentials;
    use mockito::Server;

    #[test]
    fn test_connector_metadata() {
        let connector = GitHubConnector::new();
        assert_eq!(connector.name(), "github");
        assert_eq!(connector.poll_interval(), 300);

        let oauth = connector.oauth_config();
        assert!(oauth.auth_url.contains("github.com"));
        assert!(oauth.token_url.contains("github.com"));
        assert!(oauth.scopes.contains(&"repo".to_string()));
        assert!(oauth.scopes.contains(&"notifications".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_returns_events() {
        let mut server = Server::new_async().await;

        let _repos_mock = server
            .mock("GET", "/user/repos?sort=updated&per_page=30")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{
                    "id": 1,
                    "name": "my-repo",
                    "full_name": "alice/my-repo",
                    "description": null,
                    "language": "Rust",
                    "stargazers_count": 10,
                    "forks_count": 2,
                    "open_issues_count": 1,
                    "updated_at": "2026-02-18T00:00:00Z",
                    "private": false
                }]"#,
            )
            .create_async()
            .await;

        let _issues_mock = server
            .mock("GET", "/repos/alice/my-repo/issues?state=open&per_page=10")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{
                    "id": 99,
                    "number": 5,
                    "title": "A bug",
                    "state": "open",
                    "user": {"login": "alice"},
                    "created_at": "2026-02-17T00:00:00Z",
                    "updated_at": "2026-02-18T00:00:00Z"
                }]"#,
            )
            .create_async()
            .await;

        let _notifs_mock = server
            .mock("GET", "/notifications?per_page=30")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{
                    "id": "n1",
                    "reason": "mention",
                    "unread": true,
                    "updated_at": "2026-02-18T00:00:00Z",
                    "subject": {
                        "title": "Check this",
                        "type": "Issue",
                        "url": null
                    }
                }]"#,
            )
            .create_async()
            .await;

        let connector = GitHubConnector::with_base_url(server.url());
        let credentials = Credentials {
            access_token: "test_token".to_string(),
            refresh_token: None,
            expires_at: None,
        };

        let events = connector.fetch(&credentials).await.unwrap();
        // 1 repo + 1 issue + 1 notification = 3 events
        assert_eq!(events.len(), 3);

        let repo_event = events
            .iter()
            .find(|e| e.key.as_deref() == Some("github/repo/alice/my-repo"))
            .unwrap();
        assert_eq!(repo_event.schema.as_deref(), Some("github.repository"));

        let issue_event = events
            .iter()
            .find(|e| e.key.as_deref() == Some("github/issue/alice/my-repo/5"))
            .unwrap();
        assert_eq!(issue_event.schema.as_deref(), Some("github.issue"));

        let notif_event = events
            .iter()
            .find(|e| e.key.as_deref() == Some("github/notification/n1"))
            .unwrap();
        assert_eq!(notif_event.schema.as_deref(), Some("github.notification"));
    }
}
