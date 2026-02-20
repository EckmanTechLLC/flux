# Session: GitHub Connector Transformer & Implementation

**Date:** 2026-02-18
**Task:** Phase 2 Tasks 3 + 4 — GitHub Event Transformer & GitHub Connector Implementation
**Reference:** ADR-005 lines 277-285
**Status:** ✅ COMPLETE

---

## Objective

1. Create `transformer.rs` — transform `GitHubRepo`, `GitHubNotification`, `GitHubIssue` → `FluxEvent`
2. Implement `GitHubConnector` in `connectors/github/mod.rs` — wires up API client + transformer
3. Replace `MockGitHubConnector` in `registry.rs` with real `GitHubConnector`

---

## Files Created

1. **connector-manager/src/connectors/github/transformer.rs**
   - `repo_to_event(repo)` — key: `github/repo/{full_name}`, schema: `github.repository`
   - `notification_to_event(notification)` — key: `github/notification/{id}`, schema: `github.notification`
   - `issue_to_event(owner, repo, issue)` — key: `github/issue/{owner}/{repo}/{number}`, schema: `github.issue`
   - All events: stream=`connectors`, source=`connector-manager`, UUIDv7 event_id
   - 3 tests: one per transform function

---

## Files Modified

1. **connector-manager/src/connectors/github/mod.rs**
   - Added `pub mod transformer;`
   - Added `GitHubConnector` struct with `base_url` field
   - `GitHubConnector::new()` — uses `BASE_URL`
   - `GitHubConnector::with_base_url(url)` — for test injection
   - Implemented `Connector` trait: fetch repos + per-repo issues + notifications
   - Issue fetch errors are non-fatal (warns via tracing, continues)
   - 2 tests: `test_connector_metadata`, `test_fetch_returns_events` (mock server)

2. **connector-manager/src/registry.rs**
   - Replaced `MockGitHubConnector` with `GitHubConnector::new()`
   - Removed unused imports (`anyhow`, `async_trait`, `chrono`, `uuid`, `FluxEvent`, `Credentials`)
   - Updated tests: `test_github_connector` (name, poll_interval, oauth scopes), `test_get_all_connectors`

3. **connector-manager/src/scheduler.rs**
   - Two test references to `MockGitHubConnector` updated to `GitHubConnector::new()`

4. **connector-manager/src/connectors/github/config.rs**
   - Added `static ENV_LOCK: Mutex<()>` to serialize env-var-mutating tests
   - Fixed pre-existing race condition: parallel tests competing over process env vars

---

## Design Decisions

### Entity key format
- Repos: `github/repo/{full_name}` (e.g. `github/repo/alice/my-repo`)
- Notifications: `github/notification/{id}`
- Issues: `github/issue/{owner}/{repo}/{number}`

Hierarchical format is consistent with the mock connector's `github/repo/test-repo` pattern, extended to use `full_name` (owner-qualified) for global uniqueness.

### Issue fetch strategy
Per repo, `fetch_issues(owner, name)` is called after splitting `full_name` on `/`. Issue fetch failures are non-fatal — a warning is logged and the loop continues. This prevents a single repo's rate limit or error from blocking all other repos and notifications.

### Testability via `with_base_url`
Same pattern as `GitHubClient::with_base_url` in the previous session. Keeps connector tests fully mocked without config changes.

---

## Testing

```bash
cd connector-manager && cargo test
```

**Result:** ✅ 22 tests passed, 3 doc tests passed

New tests:
- `connectors::github::transformer::tests::test_repo_to_event`
- `connectors::github::transformer::tests::test_notification_to_event`
- `connectors::github::transformer::tests::test_issue_to_event`
- `connectors::github::tests::test_connector_metadata`
- `connectors::github::tests::test_fetch_returns_events`

---

## Observations

- Pre-existing race condition in `config.rs` tests: `test_from_env_success` and `test_oauth_config` both mutate the same env vars. Adding more async tests changed thread scheduling, surfacing the race. Fixed with a static `Mutex<()>` guard.
- `MockGitHubConnector` was also referenced in `scheduler.rs` tests — required two additional replacements beyond the three scoped files.

---

## Next Steps (Phase 2)

5. **End-to-End Test** — `tests/integration/github_connector_test.rs`
   - Full OAuth flow → polling → events in Flux → query via API
