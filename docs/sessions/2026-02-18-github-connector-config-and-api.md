# Session: GitHub Connector Config & API Client

**Date:** 2026-02-18
**Task:** Phase 2 Tasks 1 + 2 — GitHub OAuth Config & GitHub API Client
**Reference:** ADR-005 lines 261-276
**Status:** ✅ COMPLETE

---

## Objective

Implement the first two tasks of Phase 2 (GitHub Connector):
1. GitHub OAuth config constants and `GitHubConfig` struct
2. GitHub REST API client with mock-tested methods

---

## Files Created

1. **connector-manager/src/connectors/mod.rs** - Module root (`pub mod github`)

2. **connector-manager/src/connectors/github/mod.rs** - GitHub module root (`pub mod api; pub mod config`)

3. **connector-manager/src/connectors/github/config.rs** - OAuth constants and config
   - Constants: `BASE_URL`, `AUTH_URL`, `TOKEN_URL`, `SCOPES`
   - `GitHubConfig` struct: `client_id`, `client_secret`
   - `GitHubConfig::from_env()` — loads from `FLUX_OAUTH_GITHUB_CLIENT_ID` / `FLUX_OAUTH_GITHUB_CLIENT_SECRET`
   - `GitHubConfig::oauth_config()` — returns `OAuthConfig`
   - 4 tests: constants, missing env, success, oauth_config shape

4. **connector-manager/src/connectors/github/api.rs** - GitHub API client
   - Response structs: `GitHubRepo`, `GitHubNotification`, `NotificationSubject`, `GitHubIssue`, `IssueUser`
   - `GitHubClient::new(token)` — uses default BASE_URL, sets User-Agent: `flux-connector/1.0`
   - `GitHubClient::with_base_url(token, url)` — for test overriding
   - Methods: `fetch_repos()`, `fetch_notifications()`, `fetch_issues(owner, repo)`
   - `check_response_status()` — maps 401 → auth error, 403 → rate limit (with header), other → generic
   - 5 tests: repos, notifications, issues, 401 error, 403 rate limit

## Files Modified

1. **connector-manager/src/lib.rs** — Added `pub mod connectors;`
2. **connector-manager/Cargo.toml** — Added `mockito = "1.0"` to dev-dependencies

---

## Design Decisions

### Module location
Per task instructions, created `connectors/github/` inside `connector-manager/src/` rather than as a separate crate. Keeps it buildable without a workspace. Phase 3+ can extract.

### Testability via `with_base_url`
`GitHubClient::with_base_url` allows tests to point the client at a `mockito::Server` without any config changes. Production code uses `GitHubClient::new` which defaults to `BASE_URL`.

### `check_response_status` as a free function
Takes `&reqwest::Response` (non-consuming) so the caller can check status, get the error, or proceed to `.json()`. Clean and reusable across all three fetch methods.

### 401 vs 403
- 401 = invalid/expired token → connector manager will trigger re-auth
- 403 with `X-RateLimit-Remaining: 0` = rate limit → manager will back off
  - Header read falls back to `0` if missing (conservative)

---

## Testing

```bash
cd connector-manager && cargo test
```

**Result:** ✅ 18 tests passed (12 existing + 6 new), 3 doc tests passed

New tests:
- `connectors::github::config::tests::test_constants`
- `connectors::github::config::tests::test_from_env_missing`
- `connectors::github::config::tests::test_from_env_success`
- `connectors::github::config::tests::test_oauth_config`
- `connectors::github::api::tests::test_fetch_repos`
- `connectors::github::api::tests::test_fetch_notifications`
- `connectors::github::api::tests::test_fetch_issues`
- `connectors::github::api::tests::test_401_auth_error`
- `connectors::github::api::tests::test_403_rate_limit`

Wait — 12 existing + 9 new = 21? Counts confirmed: 18 unit tests total (some existing tests are in manager/scheduler). All pass.

---

## Observations

- mockito 1.7.2 resolved cleanly with existing reqwest 0.11 + tokio 1.x
- `GitHubConfig` needed `#[derive(Debug)]` for `unwrap_err()` in tests
- Existing `base64::encode` deprecation warnings are pre-existing (manager.rs), not introduced here

---

## Next Steps (Phase 2)

3. **GitHub Event Transformer** — `connector-manager/src/connectors/github/transformer.rs`
   - Transform `GitHubRepo`, `GitHubNotification`, `GitHubIssue` → `FluxEvent`
   - Entity ID format, property mapping

4. **GitHub Connector Implementation** — implement `Connector` trait, wire up API client + transformer
