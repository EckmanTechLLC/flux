//! Generic connector config storage.
//!
//! Stores user-defined HTTP polling sources in SQLite. Each source defines a URL,
//! poll interval, entity key/namespace, and optional auth.
//!
//! # Credential storage
//! Generic tokens are NOT stored in this table. They are stored in the existing
//! CredentialStore under `user_id="generic"`, `connector_name=<source-id>`.
//! This reuses all encryption/access-control infrastructure without new plumbing.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Auth scheme for a generic HTTP source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthType {
    /// No authentication.
    None,
    /// `Authorization: Bearer <token>` header. Token from CredentialStore.
    BearerToken,
    /// Custom API key header. Token from CredentialStore.
    ApiKeyHeader { header_name: String },
}

/// Config for a single generic HTTP polling source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenericSourceConfig {
    /// Unique source ID (UUIDv4).
    pub id: String,
    /// Human-readable label shown in the UI.
    pub name: String,
    /// URL to poll.
    pub url: String,
    /// How often to poll (seconds).
    pub poll_interval_secs: u64,
    /// Fixed string or JSON path expression used as the Flux entity key.
    pub entity_key: String,
    /// Flux namespace to publish entities under.
    pub namespace: String,
    /// Authentication scheme (token stored separately in CredentialStore).
    pub auth_type: AuthType,
    /// When this source was created.
    pub created_at: DateTime<Utc>,
    /// Optional Flux namespace token for auth-enabled Flux instances.
    pub flux_namespace_token: Option<String>,
}

/// Persists generic source configs in SQLite.
pub struct GenericConfigStore {
    conn: Mutex<Connection>,
}

impl GenericConfigStore {
    /// Opens (or creates) the SQLite database and ensures the table exists.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open generic config DB at {}", db_path))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_table()?;
        store.migrate()?;
        Ok(store)
    }

    /// Creates the `generic_sources` table if it does not already exist.
    pub fn create_table(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS generic_sources (
                id                TEXT PRIMARY KEY,
                name              TEXT NOT NULL,
                url               TEXT NOT NULL,
                poll_interval_secs INTEGER NOT NULL,
                entity_key        TEXT NOT NULL,
                namespace         TEXT NOT NULL,
                auth_type_json    TEXT NOT NULL,
                created_at        TEXT NOT NULL,
                flux_namespace_token TEXT
            );",
        )
        .context("Failed to create generic_sources table")?;
        Ok(())
    }

    /// Adds `flux_namespace_token` column to existing databases.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute_batch(
            "ALTER TABLE generic_sources ADD COLUMN flux_namespace_token TEXT;",
        );
        if let Err(e) = result {
            if !e.to_string().contains("duplicate column") {
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Inserts a new generic source config. Fails if `id` already exists.
    pub fn insert(&self, config: &GenericSourceConfig) -> Result<()> {
        let auth_json =
            serde_json::to_string(&config.auth_type).context("Failed to serialize auth_type")?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO generic_sources
                (id, name, url, poll_interval_secs, entity_key, namespace, auth_type_json, created_at, flux_namespace_token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                config.id,
                config.name,
                config.url,
                config.poll_interval_secs as i64,
                config.entity_key,
                config.namespace,
                auth_json,
                config.created_at.to_rfc3339(),
                config.flux_namespace_token,
            ],
        )
        .context("Failed to insert generic source config")?;
        Ok(())
    }

    /// Returns a single source by ID, or `None` if not found.
    pub fn get(&self, id: &str) -> Result<Option<GenericSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, poll_interval_secs, entity_key, namespace, auth_type_json, created_at, flux_namespace_token
             FROM generic_sources WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_config(row)?))
        } else {
            Ok(None)
        }
    }

    /// Returns all source configs ordered by creation time.
    pub fn list(&self) -> Result<Vec<GenericSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, url, poll_interval_secs, entity_key, namespace, auth_type_json, created_at, flux_namespace_token
             FROM generic_sources ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(row_to_config(row).expect("row_to_config failed"))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to list generic source configs")
    }

    /// Deletes a source by ID. No-op if the ID does not exist.
    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM generic_sources WHERE id = ?1", params![id])
            .context("Failed to delete generic source config")?;
        Ok(())
    }
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<GenericSourceConfig> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let url: String = row.get(2)?;
    let poll_interval_secs: i64 = row.get(3)?;
    let entity_key: String = row.get(4)?;
    let namespace: String = row.get(5)?;
    let auth_type_json: String = row.get(6)?;
    let created_at_str: String = row.get(7)?;
    let flux_namespace_token: Option<String> = row.get(8)?;

    let auth_type: AuthType =
        serde_json::from_str(&auth_type_json).expect("Failed to deserialize auth_type");
    let created_at: DateTime<Utc> =
        created_at_str.parse().expect("Failed to parse created_at");

    Ok(GenericSourceConfig {
        id,
        name,
        url,
        poll_interval_secs: poll_interval_secs as u64,
        entity_key,
        namespace,
        auth_type,
        created_at,
        flux_namespace_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn in_memory_store() -> GenericConfigStore {
        GenericConfigStore::new(":memory:").expect("in-memory store failed")
    }

    fn sample_config(id: &str) -> GenericSourceConfig {
        GenericSourceConfig {
            id: id.to_string(),
            name: "Test Source".to_string(),
            url: "https://example.com/api".to_string(),
            poll_interval_secs: 60,
            entity_key: "test-entity".to_string(),
            namespace: "personal".to_string(),
            auth_type: AuthType::None,
            created_at: Utc::now(),
            flux_namespace_token: None,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = in_memory_store();
        let config = sample_config("abc-123");

        store.insert(&config).expect("insert failed");

        let result = store.get("abc-123").expect("get failed");
        assert!(result.is_some());
        let fetched = result.unwrap();
        assert_eq!(fetched.id, "abc-123");
        assert_eq!(fetched.name, "Test Source");
        assert_eq!(fetched.url, "https://example.com/api");
        assert_eq!(fetched.poll_interval_secs, 60);
        assert_eq!(fetched.entity_key, "test-entity");
        assert_eq!(fetched.namespace, "personal");
        assert_eq!(fetched.auth_type, AuthType::None);
    }

    #[test]
    fn test_insert_and_get_bearer_token() {
        let store = in_memory_store();
        let mut config = sample_config("bearer-src");
        config.auth_type = AuthType::BearerToken;

        store.insert(&config).expect("insert failed");

        let fetched = store.get("bearer-src").unwrap().unwrap();
        assert_eq!(fetched.auth_type, AuthType::BearerToken);
    }

    #[test]
    fn test_insert_and_get_api_key_header() {
        let store = in_memory_store();
        let mut config = sample_config("apikey-src");
        config.auth_type = AuthType::ApiKeyHeader {
            header_name: "X-API-Key".to_string(),
        };

        store.insert(&config).expect("insert failed");

        let fetched = store.get("apikey-src").unwrap().unwrap();
        assert_eq!(
            fetched.auth_type,
            AuthType::ApiKeyHeader {
                header_name: "X-API-Key".to_string()
            }
        );
    }

    #[test]
    fn test_list_configs() {
        let store = in_memory_store();

        store.insert(&sample_config("id-1")).unwrap();
        store.insert(&sample_config("id-2")).unwrap();
        store.insert(&sample_config("id-3")).unwrap();

        let configs = store.list().expect("list failed");
        assert_eq!(configs.len(), 3);
        let ids: Vec<&str> = configs.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"id-1"));
        assert!(ids.contains(&"id-2"));
        assert!(ids.contains(&"id-3"));
    }

    #[test]
    fn test_delete_config() {
        let store = in_memory_store();

        store.insert(&sample_config("del-me")).unwrap();
        assert!(store.get("del-me").unwrap().is_some());

        store.delete("del-me").expect("delete failed");
        assert!(store.get("del-me").unwrap().is_none());

        let configs = store.list().unwrap();
        assert_eq!(configs.len(), 0);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = in_memory_store();
        let result = store.get("no-such-id").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_nonexistent_is_noop() {
        let store = in_memory_store();
        // Should not error
        store.delete("ghost").unwrap();
    }
}
