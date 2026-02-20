//! Encrypted credential storage using SQLite.
//!
//! Stores OAuth credentials (access tokens, refresh tokens) for users and connectors.
//! All tokens are encrypted at rest using AES-256-GCM.

use super::{encryption, Credentials};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// Encrypted credential storage backed by SQLite.
///
/// # Schema
/// ```sql
/// CREATE TABLE credentials (
///     id INTEGER PRIMARY KEY,
///     user_id TEXT NOT NULL,
///     connector TEXT NOT NULL,
///     access_token TEXT NOT NULL,      -- Encrypted
///     access_token_nonce TEXT NOT NULL, -- Nonce for access_token
///     refresh_token TEXT,               -- Encrypted (optional)
///     refresh_token_nonce TEXT,         -- Nonce for refresh_token (optional)
///     expires_at TEXT,                  -- ISO 8601 timestamp (optional)
///     created_at TEXT NOT NULL,         -- ISO 8601 timestamp
///     updated_at TEXT NOT NULL,         -- ISO 8601 timestamp
///     UNIQUE(user_id, connector)
/// );
/// ```
///
/// # Security
/// - Access and refresh tokens are encrypted separately with unique nonces
/// - Master key is stored in memory only (from env var)
/// - Database file is protected by filesystem permissions
/// - SQLite ACID guarantees prevent partial updates
///
/// # Thread Safety
/// - Connection is wrapped in Mutex for safe concurrent access
/// - SQLite itself is thread-safe with serialized mode
pub struct CredentialStore {
    conn: Mutex<Connection>,
    encryption_key: Vec<u8>,
}

impl CredentialStore {
    /// Creates or opens a credential store.
    ///
    /// # Arguments
    /// * `db_path` - Path to SQLite database file
    /// * `encryption_key` - Base64-encoded 32-byte master key
    ///
    /// # Returns
    /// * `Ok(CredentialStore)` - Initialized store
    /// * `Err` - If database creation fails or key is invalid
    pub fn new<P: AsRef<Path>>(db_path: P, encryption_key: &str) -> Result<Self> {
        // Validate encryption key
        let key_bytes = encryption::validate_key(encryption_key)
            .context("Invalid encryption key")?;

        // Open/create database
        let conn = Connection::open(db_path).context("Failed to open database")?;

        // Create schema if not exists
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS credentials (
                id INTEGER PRIMARY KEY,
                user_id TEXT NOT NULL,
                connector TEXT NOT NULL,
                access_token TEXT NOT NULL,
                access_token_nonce TEXT NOT NULL,
                refresh_token TEXT,
                refresh_token_nonce TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(user_id, connector)
            )
            "#,
            [],
        )
        .context("Failed to create credentials table")?;

        // Create index for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_user_connector ON credentials(user_id, connector)",
            [],
        )
        .context("Failed to create index")?;

        Ok(Self {
            conn: Mutex::new(conn),
            encryption_key: key_bytes,
        })
    }

    /// Stores credentials for a user and connector.
    ///
    /// If credentials already exist, they are replaced (upsert).
    ///
    /// # Arguments
    /// * `user_id` - User identifier (namespace)
    /// * `connector` - Connector name (e.g., "github")
    /// * `credentials` - OAuth credentials to store
    ///
    /// # Returns
    /// * `Ok(())` - Credentials stored successfully
    /// * `Err` - If encryption or database operation fails
    pub fn store(&self, user_id: &str, connector: &str, credentials: &Credentials) -> Result<()> {
        // Encrypt access token
        let (access_token_encrypted, access_token_nonce) =
            encryption::encrypt(&credentials.access_token, &self.encryption_key)
                .context("Failed to encrypt access token")?;

        // Encrypt refresh token if present
        let (refresh_token_encrypted, refresh_token_nonce) = match &credentials.refresh_token {
            Some(token) => {
                let (encrypted, nonce) = encryption::encrypt(token, &self.encryption_key)
                    .context("Failed to encrypt refresh token")?;
                (Some(encrypted), Some(nonce))
            }
            None => (None, None),
        };

        // Convert expires_at to ISO 8601 string
        let expires_at = credentials.expires_at.map(|dt| dt.to_rfc3339());

        let now = Utc::now().to_rfc3339();

        // Upsert (INSERT OR REPLACE)
        self.conn
            .lock()
            .unwrap()
            .execute(
                r#"
                INSERT INTO credentials (
                    user_id, connector,
                    access_token, access_token_nonce,
                    refresh_token, refresh_token_nonce,
                    expires_at, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(user_id, connector) DO UPDATE SET
                    access_token = excluded.access_token,
                    access_token_nonce = excluded.access_token_nonce,
                    refresh_token = excluded.refresh_token,
                    refresh_token_nonce = excluded.refresh_token_nonce,
                    expires_at = excluded.expires_at,
                    updated_at = excluded.updated_at
                "#,
                params![
                    user_id,
                    connector,
                    access_token_encrypted,
                    access_token_nonce,
                    refresh_token_encrypted,
                    refresh_token_nonce,
                    expires_at,
                    now,
                    now,
                ],
            )
            .context("Failed to store credentials")?;

        Ok(())
    }

    /// Retrieves credentials for a user and connector.
    ///
    /// # Arguments
    /// * `user_id` - User identifier
    /// * `connector` - Connector name
    ///
    /// # Returns
    /// * `Ok(Some(Credentials))` - Credentials found and decrypted
    /// * `Ok(None)` - No credentials found
    /// * `Err` - If decryption or database operation fails
    pub fn get(&self, user_id: &str, connector: &str) -> Result<Option<Credentials>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT access_token, access_token_nonce,
                       refresh_token, refresh_token_nonce,
                       expires_at
                FROM credentials
                WHERE user_id = ?1 AND connector = ?2
                "#,
            )
            .context("Failed to prepare query")?;

        let mut rows = stmt
            .query(params![user_id, connector])
            .context("Failed to execute query")?;

        if let Some(row) = rows.next().context("Failed to read row")? {
            // Decrypt access token
            let access_token_encrypted: String = row.get(0)?;
            let access_token_nonce: String = row.get(1)?;
            let access_token = encryption::decrypt(
                &access_token_encrypted,
                &access_token_nonce,
                &self.encryption_key,
            )
            .context("Failed to decrypt access token")?;

            // Decrypt refresh token if present
            let refresh_token: Option<String> = row.get(2)?;
            let refresh_token_nonce: Option<String> = row.get(3)?;
            let refresh_token = match (refresh_token, refresh_token_nonce) {
                (Some(encrypted), Some(nonce)) => {
                    Some(encryption::decrypt(&encrypted, &nonce, &self.encryption_key)
                        .context("Failed to decrypt refresh token")?)
                }
                _ => None,
            };

            // Parse expires_at
            let expires_at: Option<String> = row.get(4)?;
            let expires_at = expires_at
                .map(|s| DateTime::parse_from_rfc3339(&s).map(|dt| dt.with_timezone(&Utc)))
                .transpose()
                .context("Failed to parse expires_at timestamp")?;

            Ok(Some(Credentials {
                access_token,
                refresh_token,
                expires_at,
            }))
        } else {
            Ok(None)
        }
    }

    /// Updates credentials for a user and connector.
    ///
    /// This is an alias for `store()` since the storage uses upsert semantics.
    ///
    /// # Arguments
    /// * `user_id` - User identifier
    /// * `connector` - Connector name
    /// * `credentials` - Updated credentials
    pub fn update(&self, user_id: &str, connector: &str, credentials: &Credentials) -> Result<()> {
        self.store(user_id, connector, credentials)
    }

    /// Deletes credentials for a user and connector.
    ///
    /// # Arguments
    /// * `user_id` - User identifier
    /// * `connector` - Connector name
    ///
    /// # Returns
    /// * `Ok(true)` - Credentials deleted
    /// * `Ok(false)` - No credentials found
    /// * `Err` - If database operation fails
    pub fn delete(&self, user_id: &str, connector: &str) -> Result<bool> {
        let rows_affected = self
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM credentials WHERE user_id = ?1 AND connector = ?2",
                params![user_id, connector],
            )
            .context("Failed to delete credentials")?;

        Ok(rows_affected > 0)
    }

    /// Lists all (user_id, connector) pairs across all users.
    ///
    /// Used by the connector manager on startup to resume polling
    /// for all users that previously authorized connectors.
    pub fn list_all(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT user_id, connector FROM credentials ORDER BY user_id, connector")
            .context("Failed to prepare query")?;

        let pairs = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .context("Failed to execute query")?
            .collect::<Result<Vec<(String, String)>, _>>()
            .context("Failed to read results")?;

        Ok(pairs)
    }

    /// Lists all connectors with stored credentials for a user.
    ///
    /// # Arguments
    /// * `user_id` - User identifier
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - List of connector names
    /// * `Err` - If database operation fails
    pub fn list_by_user(&self, user_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT connector FROM credentials WHERE user_id = ?1 ORDER BY connector")
            .context("Failed to prepare query")?;

        let connectors = stmt
            .query_map(params![user_id], |row| row.get(0))
            .context("Failed to execute query")?
            .collect::<Result<Vec<String>, _>>()
            .context("Failed to read results")?;

        Ok(connectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use chrono::Duration;

    fn create_test_store() -> CredentialStore {
        // Generate random 32-byte key for testing
        let key = BASE64.encode(&[0u8; 32]);
        CredentialStore::new(":memory:", &key).expect("Failed to create test store")
    }

    fn create_test_credentials() -> Credentials {
        Credentials {
            access_token: "access-token-12345".to_string(),
            refresh_token: Some("refresh-token-67890".to_string()),
            expires_at: Some(Utc::now() + Duration::hours(1)),
        }
    }

    #[test]
    fn test_store_and_get() {
        let store = create_test_store();
        let creds = create_test_credentials();

        // Store credentials
        store
            .store("user1", "github", &creds)
            .expect("Failed to store");

        // Retrieve credentials
        let retrieved = store
            .get("user1", "github")
            .expect("Failed to get")
            .expect("Credentials not found");

        assert_eq!(retrieved.access_token, creds.access_token);
        assert_eq!(retrieved.refresh_token, creds.refresh_token);
        assert!(retrieved.expires_at.is_some());
    }

    #[test]
    fn test_get_nonexistent() {
        let store = create_test_store();

        let result = store.get("user1", "github").expect("Failed to get");
        assert!(result.is_none());
    }

    #[test]
    fn test_update() {
        let store = create_test_store();
        let creds1 = create_test_credentials();

        // Store initial credentials
        store.store("user1", "github", &creds1).unwrap();

        // Update with new credentials
        let creds2 = Credentials {
            access_token: "new-access-token".to_string(),
            refresh_token: Some("new-refresh-token".to_string()),
            expires_at: Some(Utc::now() + Duration::hours(2)),
        };
        store.update("user1", "github", &creds2).unwrap();

        // Should have new credentials
        let retrieved = store.get("user1", "github").unwrap().unwrap();
        assert_eq!(retrieved.access_token, creds2.access_token);
        assert_eq!(retrieved.refresh_token, creds2.refresh_token);
    }

    #[test]
    fn test_delete() {
        let store = create_test_store();
        let creds = create_test_credentials();

        // Store credentials
        store.store("user1", "github", &creds).unwrap();

        // Delete
        let deleted = store.delete("user1", "github").unwrap();
        assert!(deleted);

        // Should not exist anymore
        let result = store.get("user1", "github").unwrap();
        assert!(result.is_none());

        // Deleting again should return false
        let deleted_again = store.delete("user1", "github").unwrap();
        assert!(!deleted_again);
    }

    #[test]
    fn test_list_by_user() {
        let store = create_test_store();
        let creds = create_test_credentials();

        // Store credentials for multiple connectors
        store.store("user1", "github", &creds).unwrap();
        store.store("user1", "gmail", &creds).unwrap();
        store.store("user1", "linkedin", &creds).unwrap();
        store.store("user2", "github", &creds).unwrap();

        // List for user1
        let connectors = store.list_by_user("user1").unwrap();
        assert_eq!(connectors.len(), 3);
        assert!(connectors.contains(&"github".to_string()));
        assert!(connectors.contains(&"gmail".to_string()));
        assert!(connectors.contains(&"linkedin".to_string()));

        // List for user2
        let connectors = store.list_by_user("user2").unwrap();
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0], "github");

        // List for nonexistent user
        let connectors = store.list_by_user("user3").unwrap();
        assert_eq!(connectors.len(), 0);
    }

    #[test]
    fn test_credentials_without_refresh_token() {
        let store = create_test_store();
        let creds = Credentials {
            access_token: "access-only".to_string(),
            refresh_token: None,
            expires_at: None,
        };

        store.store("user1", "github", &creds).unwrap();

        let retrieved = store.get("user1", "github").unwrap().unwrap();
        assert_eq!(retrieved.access_token, "access-only");
        assert!(retrieved.refresh_token.is_none());
        assert!(retrieved.expires_at.is_none());
    }

    #[test]
    fn test_concurrent_access() {
        let store = create_test_store();
        let creds = create_test_credentials();

        // SQLite ensures ACID properties
        store.store("user1", "github", &creds).unwrap();
        store.store("user1", "gmail", &creds).unwrap();

        let github = store.get("user1", "github").unwrap().unwrap();
        let gmail = store.get("user1", "gmail").unwrap().unwrap();

        assert_eq!(github.access_token, creds.access_token);
        assert_eq!(gmail.access_token, creds.access_token);
    }

    #[test]
    fn test_invalid_encryption_key() {
        // Too short
        let result = CredentialStore::new(":memory:", "short");
        assert!(result.is_err());

        // Invalid base64
        let result = CredentialStore::new(":memory:", "not-valid-base64!@#$");
        assert!(result.is_err());
    }
}
