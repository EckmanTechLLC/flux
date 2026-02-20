//! Encrypted credential storage for OAuth tokens.
//!
//! This module provides secure storage for OAuth access tokens and refresh tokens
//! using AES-256-GCM encryption backed by SQLite.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │       CredentialStore                    │
//! │  - CRUD operations                       │
//! │  - Transparent encryption/decryption     │
//! └─────────────────────────────────────────┘
//!          ↓                    ↑
//!    (encrypt)            (decrypt)
//!          ↓                    ↑
//! ┌─────────────────────────────────────────┐
//! │       Encryption Module                  │
//! │  - AES-256-GCM                           │
//! │  - Unique nonces per token               │
//! └─────────────────────────────────────────┘
//!          ↓                    ↑
//! ┌─────────────────────────────────────────┐
//! │       SQLite Database                    │
//! │  - Encrypted tokens at rest              │
//! │  - ACID guarantees                       │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```no_run
//! use flux::credentials::{CredentialStore, Credentials};
//! use chrono::{Utc, Duration};
//!
//! # fn main() -> anyhow::Result<()> {
//! // Initialize store with master key from env
//! let encryption_key = std::env::var("FLUX_ENCRYPTION_KEY")?;
//! let store = CredentialStore::new("credentials.db", &encryption_key)?;
//!
//! // Store credentials
//! let creds = Credentials {
//!     access_token: "github_access_token".to_string(),
//!     refresh_token: Some("github_refresh_token".to_string()),
//!     expires_at: Some(Utc::now() + Duration::hours(1)),
//! };
//! store.store("user1", "github", &creds)?;
//!
//! // Retrieve credentials
//! if let Some(creds) = store.get("user1", "github")? {
//!     println!("Access token: {}", creds.access_token);
//! }
//!
//! // List all connectors for a user
//! let connectors = store.list_by_user("user1")?;
//! println!("Connected: {:?}", connectors);
//!
//! // Delete credentials
//! store.delete("user1", "github")?;
//! # Ok(())
//! # }
//! ```
//!
//! # Security
//!
//! - All tokens encrypted at rest with AES-256-GCM
//! - Each token has a unique nonce (never reused)
//! - Master key must be 32 bytes (256 bits)
//! - Master key stored in memory only (from env var)
//! - Authenticated encryption (tampering detected)
//! - SQLite ACID guarantees prevent partial updates

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod encryption;
mod storage;

pub use storage::CredentialStore;

// Re-export encryption functions for testing/utilities
pub use encryption::{decrypt, encrypt, validate_key};

/// Credentials for accessing an external API.
///
/// Contains OAuth tokens and expiration information. Managed by the
/// credential store and used by connectors during fetch operations.
///
/// # Security
/// - Tokens are encrypted at rest in the credential store
/// - Never expose credentials via public APIs
/// - Refresh tokens automatically before expiration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    /// OAuth access token (used for API requests)
    pub access_token: String,

    /// OAuth refresh token (used to obtain new access tokens)
    pub refresh_token: Option<String>,

    /// When the access token expires (UTC)
    pub expires_at: Option<DateTime<Utc>>,
}
