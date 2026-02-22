// HTTP and WebSocket APIs (Tasks 4-6)

mod ingestion;
pub mod admin;
pub mod auth_middleware;
pub mod connectors;
pub mod deletion;
pub mod history;
pub mod namespace;
pub mod oauth;
pub mod query;
pub mod websocket;

pub use admin::{create_admin_router, AdminAppState};
pub use connectors::{create_connector_router, ConnectorAppState};
pub use deletion::{create_deletion_router, DeletionAppState};
pub use history::{create_history_router, HistoryAppState};
pub use ingestion::{create_router, AppState};
pub use namespace::create_namespace_router;
pub use oauth::{create_oauth_router, run_state_cleanup, OAuthAppState, StateManager};
pub use query::{create_query_router, QueryAppState};
pub use websocket::{create_ws_router, ws_handler, WsAppState};
