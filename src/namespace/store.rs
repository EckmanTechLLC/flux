//! Namespace persistence using SQLite.
//!
//! Stores registered namespaces so they survive Flux restarts.
//! `entity_count` is runtime-derived and not persisted.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::Mutex;

use super::Namespace;

/// Persists namespace records in SQLite.
pub struct NamespaceStore {
    conn: Mutex<Connection>,
}

impl NamespaceStore {
    /// Opens (or creates) the SQLite database and ensures the table exists.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open namespace DB at {}", db_path))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.create_table()?;
        Ok(store)
    }

    fn create_table(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS namespaces (
                id         TEXT PRIMARY KEY,
                name       TEXT UNIQUE NOT NULL,
                token      TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
        )
        .context("Failed to create namespaces table")?;
        Ok(())
    }

    /// Inserts a new namespace. Fails if id or name already exists.
    pub fn insert(&self, ns: &Namespace) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO namespaces (id, name, token, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![ns.id, ns.name, ns.token, ns.created_at.to_rfc3339()],
        )
        .context("Failed to insert namespace")?;
        Ok(())
    }

    /// Deletes a namespace by name. Returns Ok(()) whether or not the row exists.
    pub fn delete(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM namespaces WHERE name = ?1", params![name])
            .context("Failed to delete namespace")?;
        Ok(())
    }

    /// Returns all persisted namespaces ordered by creation time.
    pub fn load_all(&self) -> Result<Vec<Namespace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, token, created_at FROM namespaces ORDER BY created_at ASC",
            )
            .context("Failed to prepare load_all query")?;
        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let name: String = row.get(1)?;
                let token: String = row.get(2)?;
                let created_at_str: String = row.get(3)?;
                Ok((id, name, token, created_at_str))
            })
            .context("Failed to query namespaces")?;

        let mut namespaces = Vec::new();
        for row in rows {
            let (id, name, token, created_at_str) = row.context("Failed to read namespace row")?;
            let created_at = created_at_str
                .parse()
                .with_context(|| format!("Failed to parse created_at for namespace {}", id))?;
            namespaces.push(Namespace {
                id,
                name,
                token,
                created_at,
                entity_count: 0,
            });
        }
        Ok(namespaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn in_memory_store() -> NamespaceStore {
        NamespaceStore::new(":memory:").expect("in-memory store failed")
    }

    fn sample_namespace(id: &str, name: &str) -> Namespace {
        Namespace {
            id: id.to_string(),
            name: name.to_string(),
            token: "tok-abc123".to_string(),
            created_at: Utc::now(),
            entity_count: 0,
        }
    }

    #[test]
    fn test_insert_and_load_all() {
        let store = in_memory_store();
        let ns = sample_namespace("ns_abc12345", "myspace");

        store.insert(&ns).expect("insert failed");

        let loaded = store.load_all().expect("load_all failed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "ns_abc12345");
        assert_eq!(loaded[0].name, "myspace");
        assert_eq!(loaded[0].token, "tok-abc123");
        assert_eq!(loaded[0].entity_count, 0);
    }

    #[test]
    fn test_load_all_empty() {
        let store = in_memory_store();
        let loaded = store.load_all().expect("load_all failed");
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_insert_multiple_round_trip() {
        let store = in_memory_store();
        store
            .insert(&sample_namespace("ns_aaaaaaaa", "alpha"))
            .unwrap();
        store
            .insert(&sample_namespace("ns_bbbbbbbb", "beta"))
            .unwrap();
        store
            .insert(&sample_namespace("ns_cccccccc", "gamma"))
            .unwrap();

        let loaded = store.load_all().expect("load_all failed");
        assert_eq!(loaded.len(), 3);
        let names: Vec<&str> = loaded.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"gamma"));
    }

    #[test]
    fn test_duplicate_name_fails() {
        let store = in_memory_store();
        store
            .insert(&sample_namespace("ns_aaaaaaaa", "myspace"))
            .unwrap();
        let result = store.insert(&sample_namespace("ns_bbbbbbbb", "myspace"));
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_existing() {
        let store = in_memory_store();
        store
            .insert(&sample_namespace("ns_aaaaaaaa", "myspace"))
            .unwrap();

        store.delete("myspace").expect("delete should succeed");

        let loaded = store.load_all().expect("load_all failed");
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_delete_nonexistent_is_ok() {
        let store = in_memory_store();
        let result = store.delete("nonexistent");
        assert!(result.is_ok());
    }

    #[test]
    fn test_duplicate_id_fails() {
        let store = in_memory_store();
        store
            .insert(&sample_namespace("ns_aaaaaaaa", "alpha"))
            .unwrap();
        let result = store.insert(&sample_namespace("ns_aaaaaaaa", "beta"));
        assert!(result.is_err());
    }
}
