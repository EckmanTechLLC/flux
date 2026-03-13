//! i3X connector config storage.
//!
//! Stores i3X source configs in SQLite. Each source defines a base URL,
//! namespace, and Flux auth token.
//!
//! # Credential storage
//! API keys are NOT stored in this table. They are stored in the existing
//! CredentialStore under `user_id="i3x"`, `connector_name=<source-id>`
//! (i.e. key `"i3x/{source_id}"`).

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Config for a single i3X source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct I3xSourceConfig {
    /// Unique source ID (UUIDv4).
    pub id: String,
    /// Human-readable label shown in the UI.
    pub name: String,
    /// i3X endpoint base URL (e.g. `"https://demo.i3x.dev"`).
    pub base_url: String,
    /// Flux namespace to publish entities under.
    pub namespace: String,
    /// Flux auth token for auth-enabled Flux instances.
    pub flux_namespace_token: String,
    /// When this source was created.
    pub created_at: DateTime<Utc>,
}

/// Persists i3X source configs in SQLite.
pub struct I3xConfigStore {
    conn: Mutex<Connection>,
}

impl I3xConfigStore {
    /// Opens (or creates) the SQLite database and ensures the table exists.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open i3x config DB at {}", db_path))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_table()?;
        Ok(store)
    }

    /// Creates the `i3x_sources` table if it does not already exist.
    pub fn create_table(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS i3x_sources (
                id                   TEXT PRIMARY KEY,
                name                 TEXT NOT NULL,
                base_url             TEXT NOT NULL,
                namespace            TEXT NOT NULL,
                flux_namespace_token TEXT NOT NULL,
                created_at           TEXT NOT NULL
            );",
        )
        .context("Failed to create i3x_sources table")?;
        Ok(())
    }

    /// Inserts a new i3X source config. Fails if `id` already exists.
    pub fn insert(&self, config: &I3xSourceConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO i3x_sources
                (id, name, base_url, namespace, flux_namespace_token, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                config.id,
                config.name,
                config.base_url,
                config.namespace,
                config.flux_namespace_token,
                config.created_at.to_rfc3339(),
            ],
        )
        .context("Failed to insert i3x source config")?;
        Ok(())
    }

    /// Returns a single source by ID, or `None` if not found.
    pub fn get(&self, id: &str) -> Result<Option<I3xSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, namespace, flux_namespace_token, created_at
             FROM i3x_sources WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_config(row)?))
        } else {
            Ok(None)
        }
    }

    /// Returns all source configs ordered by creation time.
    pub fn list(&self) -> Result<Vec<I3xSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, namespace, flux_namespace_token, created_at
             FROM i3x_sources ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(row_to_config(row).expect("row_to_config failed"))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to list i3x source configs")
    }

    /// Deletes a source by ID. No-op if the ID does not exist.
    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM i3x_sources WHERE id = ?1", params![id])
            .context("Failed to delete i3x source config")?;
        Ok(())
    }
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<I3xSourceConfig> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let base_url: String = row.get(2)?;
    let namespace: String = row.get(3)?;
    let flux_namespace_token: String = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let created_at: DateTime<Utc> = created_at_str.parse().expect("Failed to parse created_at");
    Ok(I3xSourceConfig {
        id,
        name,
        base_url,
        namespace,
        flux_namespace_token,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn in_memory_store() -> I3xConfigStore {
        I3xConfigStore::new(":memory:").expect("in-memory store failed")
    }

    fn sample_config(id: &str) -> I3xSourceConfig {
        I3xSourceConfig {
            id: id.to_string(),
            name: "Test i3X Source".to_string(),
            base_url: "https://demo.i3x.dev".to_string(),
            namespace: "flux-manufacturing".to_string(),
            flux_namespace_token: "tok-abc123".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = in_memory_store();
        let config = sample_config("src-001");
        store.insert(&config).expect("insert failed");

        let result = store.get("src-001").expect("get failed");
        assert!(result.is_some());
        let fetched = result.unwrap();
        assert_eq!(fetched.id, "src-001");
        assert_eq!(fetched.name, "Test i3X Source");
        assert_eq!(fetched.base_url, "https://demo.i3x.dev");
        assert_eq!(fetched.namespace, "flux-manufacturing");
        assert_eq!(fetched.flux_namespace_token, "tok-abc123");
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
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let store = in_memory_store();
        assert!(store.get("no-such-id").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent_is_noop() {
        let store = in_memory_store();
        store.delete("ghost").unwrap();
    }
}
