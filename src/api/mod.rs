// HTTP and WebSocket APIs (Tasks 4-6)

mod ingestion;
pub mod auth_middleware;
pub mod deletion;
pub mod namespace;
pub mod query;
pub mod websocket;

pub use deletion::{create_deletion_router, DeletionAppState};
pub use ingestion::{create_router, AppState};
pub use namespace::create_namespace_router;
pub use query::{create_query_router, QueryAppState};
pub use websocket::{ws_handler, WsAppState};
