//! Flux Connector Manager - Interface and types for external API connectors.
//!
//! This crate defines the standard interface that all Flux connectors must implement.
//! Connectors integrate external APIs (GitHub, Gmail, etc.) with Flux by polling
//! for data and transforming it into Flux events.
//!
//! # Architecture
//!
//! ```text
//! External API (GitHub, Gmail, etc.)
//!          ↓
//!     OAuth (user authorizes)
//!          ↓
//! ┌─────────────────────────────────────────┐
//! │       Connector (implements trait)       │
//! │  - Poll external API                     │
//! │  - Transform to Flux events              │
//! └─────────────────────────────────────────┘
//!          ↓
//! ┌─────────────────────────────────────────┐
//! │       Connector Manager                  │
//! │  - Schedule polling                      │
//! │  - Manage credentials                    │
//! │  - Publish events to Flux                │
//! └─────────────────────────────────────────┘
//!          ↓
//!       Flux Core
//! ```
//!
//! # Core Types
//!
//! - [`Connector`] - Trait that all connectors must implement
//! - [`OAuthConfig`] - OAuth configuration (auth URL, token URL, scopes)
//! - [`Credentials`] - OAuth credentials (access token, refresh token)
//! - [`FluxEvent`] - Re-exported from flux crate (event format)
//!
//! # Creating a Connector
//!
//! ```no_run
//! use connector_manager::{Connector, OAuthConfig, Credentials};
//! use async_trait::async_trait;
//! use anyhow::Result;
//! use flux::FluxEvent;
//!
//! struct MyConnector;
//!
//! #[async_trait]
//! impl Connector for MyConnector {
//!     fn name(&self) -> &str {
//!         "myservice"
//!     }
//!
//!     fn oauth_config(&self) -> OAuthConfig {
//!         OAuthConfig {
//!             auth_url: "https://api.example.com/oauth/authorize".to_string(),
//!             token_url: "https://api.example.com/oauth/token".to_string(),
//!             scopes: vec!["read".to_string()],
//!         }
//!     }
//!
//!     async fn fetch(&self, credentials: &Credentials) -> Result<Vec<FluxEvent>> {
//!         // 1. Use credentials.access_token to authenticate
//!         // 2. Fetch data from external API
//!         // 3. Transform to Flux events
//!         // 4. Return events
//!         Ok(vec![])
//!     }
//!
//!     fn poll_interval(&self) -> u64 {
//!         300 // 5 minutes
//!     }
//! }
//! ```

mod connector;
mod types;
pub mod api;
pub mod connectors;
pub mod generic_config;
pub mod manager;
pub mod named_config;
pub mod registry;
pub mod runners;

// Re-export public types
pub use connector::Connector;
pub use manager::ConnectorManager;
pub use runners::builtin::{ConnectorScheduler, ConnectorStatus};
pub use types::OAuthConfig;

// Re-export FluxEvent and Credentials from flux crate for convenience
pub use flux::credentials::Credentials;
pub use flux::FluxEvent;
