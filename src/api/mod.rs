// HTTP and WebSocket APIs (Tasks 4-6)

mod ingestion;
pub mod websocket;

pub use ingestion::{create_router, AppState};
pub use websocket::{ws_handler, WsAppState};
