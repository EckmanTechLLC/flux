// Configuration
pub mod config;

// Event model and validation
pub mod event;

// Re-export FluxEvent for external crates
pub use event::FluxEvent;

// State engine and entity management
pub mod state;

// HTTP and WebSocket APIs
pub mod api;

// NATS client integration
pub mod nats;

// Subscription management
pub mod subscription;

// Snapshot and persistence
pub mod snapshot;

// Namespace and multi-tenancy
pub mod namespace;

// Authentication and authorization
pub mod auth;

// Entity ID parsing
pub mod entity;

// Connector credential storage
pub mod credentials;

// Rate limiting (ADR-006)
pub mod rate_limit;
