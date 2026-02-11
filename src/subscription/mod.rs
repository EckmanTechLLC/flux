// WebSocket subscription management (Task 5)

pub mod manager;
pub mod protocol;

pub use manager::ConnectionManager;
pub use protocol::{ClientMessage, StateUpdateMessage};
