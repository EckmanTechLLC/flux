//! Named connector config storage.
//!
//! Stores Singer tap sources in SQLite. Each source defines a tap name,
//! namespace, entity key field, tap config JSON, and poll interval.
//!
//! # Credential storage
//! Tap config (including credentials) is stored in `config_json` as a JSON
//! string. It is written to a temp file at runtime with 0600 permissions
//! and removed after the tap exits.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Config for a single named Singer tap source.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedSourceConfig {
    /// Unique source ID (UUIDv4).
    pub id: String,
    /// Tap command name (e.g. `"tap-github"`).
    pub tap_name: String,
    /// Flux namespace to publish entities under.
    pub namespace: String,
    /// Record field to use as the Flux entity key (e.g. `"id"`, `"name"`).
    pub entity_key_field: String,
    /// Tap configuration JSON (credentials + settings).
    /// Written to `/tmp/flux-tap-{id}-config.json` at runtime.
    pub config_json: String,
    /// How often to re-run the tap after it exits (seconds).
    pub poll_interval_secs: u64,
    /// When this source was created.
    pub created_at: DateTime<Utc>,
    /// Optional Flux namespace token for auth-enabled Flux instances.
    pub flux_namespace_token: Option<String>,
}

/// Persists named source configs in SQLite.
pub struct NamedConfigStore {
    conn: Mutex<Connection>,
}

impl NamedConfigStore {
    /// Opens (or creates) the SQLite database and ensures the table exists.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open named config DB at {}", db_path))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_table()?;
        store.migrate()?;
        Ok(store)
    }

    /// Creates the `named_sources` table if it does not already exist.
    pub fn create_table(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS named_sources (
                id                  TEXT PRIMARY KEY,
                tap_name            TEXT NOT NULL,
                namespace           TEXT NOT NULL,
                entity_key_field    TEXT NOT NULL,
                config_json         TEXT NOT NULL,
                poll_interval_secs  INTEGER NOT NULL,
                created_at          TEXT NOT NULL,
                flux_namespace_token TEXT
            );",
        )
        .context("Failed to create named_sources table")?;
        Ok(())
    }

    /// Adds `flux_namespace_token` column to existing databases.
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let result = conn.execute_batch(
            "ALTER TABLE named_sources ADD COLUMN flux_namespace_token TEXT;",
        );
        if let Err(e) = result {
            if !e.to_string().contains("duplicate column") {
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Inserts a new named source config. Fails if `id` already exists.
    pub fn insert(&self, config: &NamedSourceConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO named_sources
                (id, tap_name, namespace, entity_key_field, config_json, poll_interval_secs, created_at, flux_namespace_token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                config.id,
                config.tap_name,
                config.namespace,
                config.entity_key_field,
                config.config_json,
                config.poll_interval_secs as i64,
                config.created_at.to_rfc3339(),
                config.flux_namespace_token,
            ],
        )
        .context("Failed to insert named source config")?;
        Ok(())
    }

    /// Returns a single source by ID, or `None` if not found.
    pub fn get(&self, id: &str) -> Result<Option<NamedSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tap_name, namespace, entity_key_field, config_json, poll_interval_secs, created_at, flux_namespace_token
             FROM named_sources WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_config(row)?))
        } else {
            Ok(None)
        }
    }

    /// Returns all source configs ordered by creation time.
    pub fn list(&self) -> Result<Vec<NamedSourceConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, tap_name, namespace, entity_key_field, config_json, poll_interval_secs, created_at, flux_namespace_token
             FROM named_sources ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(row_to_config(row).expect("row_to_config failed"))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to list named source configs")
    }

    /// Deletes a source by ID. No-op if the ID does not exist.
    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM named_sources WHERE id = ?1", params![id])
            .context("Failed to delete named source config")?;
        Ok(())
    }
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<NamedSourceConfig> {
    let id: String = row.get(0)?;
    let tap_name: String = row.get(1)?;
    let namespace: String = row.get(2)?;
    let entity_key_field: String = row.get(3)?;
    let config_json: String = row.get(4)?;
    let poll_interval_secs: i64 = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let flux_namespace_token: Option<String> = row.get(7)?;
    let created_at: DateTime<Utc> = created_at_str.parse().expect("Failed to parse created_at");
    Ok(NamedSourceConfig {
        id,
        tap_name,
        namespace,
        entity_key_field,
        config_json,
        poll_interval_secs: poll_interval_secs as u64,
        created_at,
        flux_namespace_token,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn in_memory_store() -> NamedConfigStore {
        NamedConfigStore::new(":memory:").expect("in-memory store failed")
    }

    fn sample_config(id: &str) -> NamedSourceConfig {
        NamedSourceConfig {
            id: id.to_string(),
            tap_name: "tap-github".to_string(),
            namespace: "personal".to_string(),
            entity_key_field: "id".to_string(),
            config_json: r#"{"access_token": "ghp_test"}"#.to_string(),
            poll_interval_secs: 3600,
            created_at: Utc::now(),
            flux_namespace_token: None,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = in_memory_store();
        let config = sample_config("src-001");
        store.insert(&config).unwrap();

        let result = store.get("src-001").unwrap();
        assert!(result.is_some());
        let fetched = result.unwrap();
        assert_eq!(fetched.id, "src-001");
        assert_eq!(fetched.tap_name, "tap-github");
        assert_eq!(fetched.namespace, "personal");
        assert_eq!(fetched.entity_key_field, "id");
        assert_eq!(fetched.poll_interval_secs, 3600);
        assert_eq!(fetched.config_json, r#"{"access_token": "ghp_test"}"#);
    }

    #[test]
    fn test_list_configs() {
        let store = in_memory_store();
        store.insert(&sample_config("id-1")).unwrap();
        store.insert(&sample_config("id-2")).unwrap();

        let configs = store.list().unwrap();
        assert_eq!(configs.len(), 2);
        let ids: Vec<&str> = configs.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"id-1"));
        assert!(ids.contains(&"id-2"));
    }

    #[test]
    fn test_delete_config() {
        let store = in_memory_store();
        store.insert(&sample_config("del-me")).unwrap();
        assert!(store.get("del-me").unwrap().is_some());

        store.delete("del-me").unwrap();
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
