use anyhow::{anyhow, Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use super::config::BASE_URL;

/// GitHub repository.
#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub open_issues_count: u64,
    pub updated_at: String,
    pub private: bool,
}

/// Subject of a GitHub notification.
#[derive(Debug, Deserialize)]
pub struct NotificationSubject {
    pub title: String,
    #[serde(rename = "type")]
    pub subject_type: String,
    pub url: Option<String>,
}

/// GitHub notification.
#[derive(Debug, Deserialize)]
pub struct GitHubNotification {
    pub id: String,
    pub reason: String,
    pub unread: bool,
    pub updated_at: String,
    pub subject: NotificationSubject,
}

/// Author of a GitHub issue.
#[derive(Debug, Deserialize)]
pub struct IssueUser {
    pub login: String,
}

/// GitHub issue.
#[derive(Debug, Deserialize)]
pub struct GitHubIssue {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub state: String,
    pub user: IssueUser,
    pub created_at: String,
    pub updated_at: String,
}

/// HTTP client for the GitHub REST API.
///
/// Authenticates with a Bearer token and sets a User-Agent header.
pub struct GitHubClient {
    access_token: String,
    http_client: Client,
    base_url: String,
}

impl GitHubClient {
    /// Create a client using the default GitHub API base URL.
    pub fn new(access_token: String) -> Self {
        Self::with_base_url(access_token, BASE_URL.to_string())
    }

    /// Create a client with a custom base URL (for testing with a mock server).
    pub fn with_base_url(access_token: String, base_url: String) -> Self {
        let http_client = Client::builder()
            .user_agent("flux-connector/1.0")
            .build()
            .expect("Failed to build HTTP client");
        Self {
            access_token,
            http_client,
            base_url,
        }
    }

    /// Fetch the authenticated user's repositories (sorted by last updated).
    pub async fn fetch_repos(&self) -> Result<Vec<GitHubRepo>> {
        let url = format!("{}/user/repos?sort=updated&per_page=30", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Failed to send fetch_repos request")?;

        check_response_status(&response)?;
        response
            .json::<Vec<GitHubRepo>>()
            .await
            .context("Failed to parse repos response")
    }

    /// Fetch the authenticated user's notifications.
    pub async fn fetch_notifications(&self) -> Result<Vec<GitHubNotification>> {
        let url = format!("{}/notifications?per_page=30", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Failed to send fetch_notifications request")?;

        check_response_status(&response)?;
        response
            .json::<Vec<GitHubNotification>>()
            .await
            .context("Failed to parse notifications response")
    }

    /// Fetch open issues for a repository.
    pub async fn fetch_issues(&self, owner: &str, repo: &str) -> Result<Vec<GitHubIssue>> {
        let url = format!(
            "{}/repos/{}/{}/issues?state=open&per_page=10",
            self.base_url, owner, repo
        );
        let response = self
            .http_client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .context("Failed to send fetch_issues request")?;

        check_response_status(&response)?;
        response
            .json::<Vec<GitHubIssue>>()
            .await
            .context("Failed to parse issues response")
    }
}

/// Check the response status and map known error codes to descriptive errors.
///
/// - 401 → auth error (token expired or invalid)
/// - 403 → rate limit (logs X-RateLimit-Remaining)
/// - Other non-2xx → generic API error
fn check_response_status(response: &reqwest::Response) -> Result<()> {
    match response.status() {
        StatusCode::UNAUTHORIZED => Err(anyhow!("GitHub auth error: token expired or invalid")),
        StatusCode::FORBIDDEN => {
            let remaining = response
                .headers()
                .get("X-RateLimit-Remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            Err(anyhow!(
                "GitHub rate limit exceeded (X-RateLimit-Remaining: {})",
                remaining
            ))
        }
        s if !s.is_success() => Err(anyhow!("GitHub API error: {}", s)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn test_fetch_repos() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/user/repos?sort=updated&per_page=30")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {
                        "id": 12345,
                        "name": "test-repo",
                        "full_name": "testuser/test-repo",
                        "description": "A test repository",
                        "language": "Rust",
                        "stargazers_count": 42,
                        "forks_count": 10,
                        "open_issues_count": 5,
                        "updated_at": "2026-02-17T12:00:00Z",
                        "private": false
                    }
                ]"#,
            )
            .create_async()
            .await;

        let client = GitHubClient::with_base_url("test_token".to_string(), server.url());
        let repos = client.fetch_repos().await.unwrap();

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "test-repo");
        assert_eq!(repos[0].full_name, "testuser/test-repo");
        assert_eq!(repos[0].stargazers_count, 42);
        assert_eq!(repos[0].language.as_deref(), Some("Rust"));
        assert!(!repos[0].private);
    }

    #[tokio::test]
    async fn test_fetch_notifications() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/notifications?per_page=30")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {
                        "id": "1",
                        "reason": "mention",
                        "unread": true,
                        "updated_at": "2026-02-17T12:00:00Z",
                        "subject": {
                            "title": "Fix the bug",
                            "type": "Issue",
                            "url": "https://api.github.com/repos/testuser/test-repo/issues/1"
                        }
                    }
                ]"#,
            )
            .create_async()
            .await;

        let client = GitHubClient::with_base_url("test_token".to_string(), server.url());
        let notifications = client.fetch_notifications().await.unwrap();

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].id, "1");
        assert_eq!(notifications[0].reason, "mention");
        assert!(notifications[0].unread);
        assert_eq!(notifications[0].subject.title, "Fix the bug");
        assert_eq!(notifications[0].subject.subject_type, "Issue");
    }

    #[tokio::test]
    async fn test_fetch_issues() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/repos/testuser/test-repo/issues?state=open&per_page=10")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                    {
                        "id": 98765,
                        "number": 42,
                        "title": "Bug: something broken",
                        "state": "open",
                        "user": {"login": "testuser"},
                        "created_at": "2026-02-17T10:00:00Z",
                        "updated_at": "2026-02-17T12:00:00Z"
                    }
                ]"#,
            )
            .create_async()
            .await;

        let client = GitHubClient::with_base_url("test_token".to_string(), server.url());
        let issues = client.fetch_issues("testuser", "test-repo").await.unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 42);
        assert_eq!(issues[0].title, "Bug: something broken");
        assert_eq!(issues[0].user.login, "testuser");
    }

    #[tokio::test]
    async fn test_401_auth_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/user/repos?sort=updated&per_page=30")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "Bad credentials"}"#)
            .create_async()
            .await;

        let client = GitHubClient::with_base_url("expired_token".to_string(), server.url());
        let err = client.fetch_repos().await.unwrap_err();
        assert!(err.to_string().contains("token expired or invalid"));
    }

    #[tokio::test]
    async fn test_403_rate_limit() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("GET", "/user/repos?sort=updated&per_page=30")
            .with_status(403)
            .with_header("X-RateLimit-Remaining", "0")
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "API rate limit exceeded"}"#)
            .create_async()
            .await;

        let client = GitHubClient::with_base_url("test_token".to_string(), server.url());
        let err = client.fetch_repos().await.unwrap_err();
        assert!(err.to_string().contains("rate limit exceeded"));
        assert!(err.to_string().contains("X-RateLimit-Remaining: 0"));
    }
}
