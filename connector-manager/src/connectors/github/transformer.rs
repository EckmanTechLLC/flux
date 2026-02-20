use chrono::Utc;
use flux::FluxEvent;
use uuid::Uuid;

use super::api::{GitHubIssue, GitHubNotification, GitHubRepo};

/// Transform a GitHub repository into a Flux event.
///
/// Entity key: `github/repo/{full_name}`
pub fn repo_to_event(repo: &GitHubRepo) -> FluxEvent {
    FluxEvent {
        event_id: Some(Uuid::now_v7().to_string()),
        stream: "connectors".to_string(),
        source: "connector-manager".to_string(),
        timestamp: Utc::now().timestamp_millis(),
        key: Some(format!("github/repo/{}", repo.full_name)),
        schema: Some("github.repository".to_string()),
        payload: serde_json::json!({
            "entity_id": format!("github/repo/{}", repo.full_name),
            "properties": {
                "name": repo.name,
                "full_name": repo.full_name,
                "description": repo.description,
                "language": repo.language,
                "stars": repo.stargazers_count,
                "forks": repo.forks_count,
                "open_issues": repo.open_issues_count,
                "private": repo.private,
                "updated_at": repo.updated_at,
            }
        }),
    }
}

/// Transform a GitHub notification into a Flux event.
///
/// Entity key: `github/notification/{id}`
pub fn notification_to_event(notification: &GitHubNotification) -> FluxEvent {
    FluxEvent {
        event_id: Some(Uuid::now_v7().to_string()),
        stream: "connectors".to_string(),
        source: "connector-manager".to_string(),
        timestamp: Utc::now().timestamp_millis(),
        key: Some(format!("github/notification/{}", notification.id)),
        schema: Some("github.notification".to_string()),
        payload: serde_json::json!({
            "entity_id": format!("github/notification/{}", notification.id),
            "properties": {
                "id": notification.id,
                "reason": notification.reason,
                "unread": notification.unread,
                "updated_at": notification.updated_at,
                "subject_title": notification.subject.title,
                "subject_type": notification.subject.subject_type,
                "subject_url": notification.subject.url,
            }
        }),
    }
}

/// Transform a GitHub issue into a Flux event.
///
/// Entity key: `github/issue/{owner}/{repo}/{number}`
pub fn issue_to_event(owner: &str, repo: &str, issue: &GitHubIssue) -> FluxEvent {
    FluxEvent {
        event_id: Some(Uuid::now_v7().to_string()),
        stream: "connectors".to_string(),
        source: "connector-manager".to_string(),
        timestamp: Utc::now().timestamp_millis(),
        key: Some(format!("github/issue/{}/{}/{}", owner, repo, issue.number)),
        schema: Some("github.issue".to_string()),
        payload: serde_json::json!({
            "entity_id": format!("github/issue/{}/{}/{}", owner, repo, issue.number),
            "properties": {
                "number": issue.number,
                "title": issue.title,
                "state": issue.state,
                "author": issue.user.login,
                "created_at": issue.created_at,
                "updated_at": issue.updated_at,
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::github::api::{
        GitHubIssue, GitHubNotification, GitHubRepo, IssueUser, NotificationSubject,
    };

    fn make_repo() -> GitHubRepo {
        GitHubRepo {
            id: 1,
            name: "test-repo".to_string(),
            full_name: "testuser/test-repo".to_string(),
            description: Some("A test repo".to_string()),
            language: Some("Rust".to_string()),
            stargazers_count: 42,
            forks_count: 10,
            open_issues_count: 5,
            updated_at: "2026-02-18T00:00:00Z".to_string(),
            private: false,
        }
    }

    fn make_notification() -> GitHubNotification {
        GitHubNotification {
            id: "notif-1".to_string(),
            reason: "mention".to_string(),
            unread: true,
            updated_at: "2026-02-18T00:00:00Z".to_string(),
            subject: NotificationSubject {
                title: "Fix the bug".to_string(),
                subject_type: "Issue".to_string(),
                url: Some(
                    "https://api.github.com/repos/testuser/test-repo/issues/1".to_string(),
                ),
            },
        }
    }

    fn make_issue() -> GitHubIssue {
        GitHubIssue {
            id: 99,
            number: 7,
            title: "Something is broken".to_string(),
            state: "open".to_string(),
            user: IssueUser {
                login: "testuser".to_string(),
            },
            created_at: "2026-02-18T00:00:00Z".to_string(),
            updated_at: "2026-02-18T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_repo_to_event() {
        let repo = make_repo();
        let event = repo_to_event(&repo);

        assert_eq!(event.stream, "connectors");
        assert_eq!(event.source, "connector-manager");
        assert_eq!(event.key.unwrap(), "github/repo/testuser/test-repo");
        assert_eq!(event.schema.unwrap(), "github.repository");
        assert!(event.event_id.is_some());
        assert_eq!(event.payload["properties"]["full_name"], "testuser/test-repo");
        assert_eq!(event.payload["properties"]["stars"], 42);
        assert_eq!(event.payload["properties"]["language"], "Rust");
        assert_eq!(event.payload["properties"]["open_issues"], 5);
        assert_eq!(event.payload["properties"]["private"], false);
    }

    #[test]
    fn test_notification_to_event() {
        let notif = make_notification();
        let event = notification_to_event(&notif);

        assert_eq!(event.key.unwrap(), "github/notification/notif-1");
        assert_eq!(event.schema.unwrap(), "github.notification");
        assert_eq!(event.payload["properties"]["reason"], "mention");
        assert_eq!(event.payload["properties"]["unread"], true);
        assert_eq!(event.payload["properties"]["subject_type"], "Issue");
        assert_eq!(event.payload["properties"]["subject_title"], "Fix the bug");
    }

    #[test]
    fn test_issue_to_event() {
        let issue = make_issue();
        let event = issue_to_event("testuser", "test-repo", &issue);

        assert_eq!(event.key.unwrap(), "github/issue/testuser/test-repo/7");
        assert_eq!(event.schema.unwrap(), "github.issue");
        assert_eq!(event.payload["properties"]["number"], 7);
        assert_eq!(event.payload["properties"]["title"], "Something is broken");
        assert_eq!(event.payload["properties"]["author"], "testuser");
        assert_eq!(event.payload["properties"]["state"], "open");
    }
}
