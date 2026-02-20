// Integration tests for WebSocket auth enforcement (ADR-006 Session 4)
//
// Auth is enforced as a tower middleware (ws_auth) that runs BEFORE WebSocket
// upgrade extraction. This allows 401 to be returned cleanly without a full
// WebSocket handshake.
//
// Note: Tests use tower::ServiceExt::oneshot. When auth passes, requests reach
// the WebSocketUpgrade extractor, which returns 426 (no hyper OnUpgrade extension
// in test requests). This is a test-environment artifact — in production the
// server returns 101. The tests verify the auth decision (401 vs non-401), not
// the WebSocket upgrade itself.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use flux::{
    api::{create_ws_router, WsAppState},
    namespace::NamespaceRegistry,
    state::StateEngine,
};
use std::sync::Arc;
use tower::ServiceExt;

fn make_router(auth_enabled: bool, registry: Arc<NamespaceRegistry>) -> Router {
    let state = Arc::new(WsAppState {
        state_engine: Arc::new(StateEngine::new()),
        namespace_registry: registry,
        auth_enabled,
    });
    create_ws_router(state)
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

// ── auth_enabled=false: all requests pass through ────────────────────────────

#[tokio::test]
async fn test_auth_disabled_no_token_allowed() {
    let app = make_router(false, Arc::new(NamespaceRegistry::new()));
    let resp = app.oneshot(get_request("/api/ws")).await.unwrap();
    // Middleware passes; WebSocket extractor fails with 426 (test artifact, not 401)
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── auth_enabled=true, missing token → 401 ───────────────────────────────────

#[tokio::test]
async fn test_auth_enabled_no_token_returns_401() {
    let app = make_router(true, Arc::new(NamespaceRegistry::new()));
    let resp = app.oneshot(get_request("/api/ws")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── auth_enabled=true, invalid token → 401 ───────────────────────────────────

#[tokio::test]
async fn test_auth_enabled_invalid_token_returns_401() {
    let app = make_router(true, Arc::new(NamespaceRegistry::new()));
    let resp = app
        .oneshot(get_request("/api/ws?token=not-a-real-token"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── auth_enabled=true, valid token → auth passes ─────────────────────────────

#[tokio::test]
async fn test_auth_enabled_valid_token_not_rejected() {
    let registry = Arc::new(NamespaceRegistry::new());
    let ns = registry.register("testns").unwrap();
    let app = make_router(true, Arc::clone(&registry));
    let uri = format!("/api/ws?token={}", ns.token);
    let resp = app.oneshot(get_request(&uri)).await.unwrap();
    // Middleware passes (auth ok); WebSocket extractor returns 426 (test artifact)
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
}
