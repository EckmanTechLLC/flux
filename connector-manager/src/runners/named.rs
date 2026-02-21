//! Named connector runner (Singer tap subprocess).
//!
//! # Tap catalog (Phase 3B Task 1)
//! `TapCatalogStore` fetches and caches the Meltano Hub extractor list.
//!
//! # Singer runner (Phase 3B Task 2)
//! `NamedRunner` spawns Singer tap subprocesses, parses their stdout, and
//! publishes Flux events. State files persist incremental sync bookmarks
//! between runs.

use crate::named_config::{NamedConfigStore, NamedSourceConfig};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, warn};

const MELTANO_INDEX_URL: &str =
    "https://hub.meltano.com/meltano/api/v1/plugins/extractors/index";

/// 24 hours in seconds.
const CACHE_TTL_SECS: i64 = 86_400;

// ---------------------------------------------------------------------------
// Tap catalog types (Phase 3B Task 1)
// ---------------------------------------------------------------------------

/// A single entry in the Meltano Hub tap catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapCatalogEntry {
    /// Tap name and pip install package (e.g. `"tap-github"`).
    pub name: String,
    /// Human-readable label derived from the tap name (e.g. `"Github"`).
    pub label: String,
    /// Description (empty for index-only entries; enriched in Session 2).
    pub description: String,
    /// pip package to `pip install` (equals `name` for most taps).
    pub pip_url: String,
    /// Logo URL from Meltano Hub.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Singer runner types (Phase 3B Task 2)
// ---------------------------------------------------------------------------

/// Runtime status for a single named Singer tap source.
#[derive(Clone, Debug)]
pub struct NamedStatus {
    pub source_id: String,
    pub tap_name: String,
    /// Time the most recent run started.
    pub last_run: Option<DateTime<Utc>>,
    /// Error message from the most recent run, if any.
    pub last_error: Option<String>,
    /// Total number of completed runs (success or failure).
    pub restart_count: u32,
}

/// Named connector runner — manages Singer tap subprocesses.
///
/// Each configured source runs in a background tokio task that:
/// 1. Writes tap config to a temp file (`/tmp/flux-tap-{id}-config.json`, mode 0600)
/// 2. Optionally passes a state file (`/tmp/flux-tap-{id}-state.json`) for incremental sync
/// 3. Spawns the tap subprocess and reads its stdout line by line
/// 4. Parses Singer `RECORD` messages → Flux events, `STATE` messages → state file
/// 5. After the tap exits, waits `poll_interval_secs`, then repeats
pub struct NamedRunner {
    pub store: Arc<NamedConfigStore>,
    pub flux_api_url: String,
    task_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    status_map: Arc<Mutex<HashMap<String, NamedStatus>>>,
}

impl NamedRunner {
    pub fn new(store: Arc<NamedConfigStore>, flux_api_url: String) -> Self {
        Self {
            store,
            flux_api_url,
            task_handles: Mutex::new(HashMap::new()),
            status_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts a polling loop for the given Singer tap source.
    ///
    /// Spawns a background task that runs the tap immediately, then reschedules
    /// it after `poll_interval_secs`. The task runs until `stop_source` is called.
    pub async fn start_source(&self, config: &NamedSourceConfig) -> Result<()> {
        {
            let mut map = self.status_map.lock().unwrap();
            map.entry(config.id.clone()).or_insert_with(|| NamedStatus {
                source_id: config.id.clone(),
                tap_name: config.tap_name.clone(),
                last_run: None,
                last_error: None,
                restart_count: 0,
            });
        }

        let config_owned = config.clone();
        let flux_url = self.flux_api_url.clone();
        let status_map = Arc::clone(&self.status_map);
        let handle = tokio::spawn(run_tap_loop(config_owned, flux_url, status_map));

        let mut handles = self.task_handles.lock().unwrap();
        handles.insert(config.id.clone(), handle);
        info!(source_id = %config.id, tap = %config.tap_name, "Named source started");
        Ok(())
    }

    /// Aborts the polling task and removes temp files for the given source.
    pub async fn stop_source(&self, source_id: &str) -> Result<()> {
        let handle = {
            let mut handles = self.task_handles.lock().unwrap();
            handles.remove(source_id)
        };
        if let Some(h) = handle {
            h.abort();
        }
        // Best-effort cleanup of temp files
        for path in [
            format!("/tmp/flux-tap-{}-config.json", source_id),
            format!("/tmp/flux-tap-{}-state.json", source_id),
        ] {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!(path = %path, error = %e, "Failed to remove tap temp file");
                }
            }
        }
        info!(source_id = %source_id, "Named source stopped");
        Ok(())
    }

    /// Returns current status for all named sources.
    pub fn status(&self) -> Vec<NamedStatus> {
        let map = self.status_map.lock().unwrap();
        map.values().cloned().collect()
    }

    /// Triggers an immediate one-shot tap run (fire and forget).
    ///
    /// Returns `Err` if the source is not found in the config store.
    /// The run result is recorded in `status_map` when it completes.
    pub async fn trigger_sync(&self, source_id: &str) -> Result<()> {
        let config = self
            .store
            .get(source_id)?
            .ok_or_else(|| anyhow::anyhow!("Named source {} not found", source_id))?;
        let flux_url = self.flux_api_url.clone();
        let status_map = Arc::clone(&self.status_map);
        tokio::spawn(async move {
            let id = config.id.clone();
            let tap = config.tap_name.clone();
            info!(source_id = %id, tap = %tap, "Manual sync triggered");
            {
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&id) {
                    s.last_run = Some(Utc::now());
                }
            }
            match run_tap_once(&config, &flux_url).await {
                Ok(()) => {
                    info!(source_id = %id, tap = %tap, "Manual sync complete");
                    let mut map = status_map.lock().unwrap();
                    if let Some(s) = map.get_mut(&id) {
                        s.last_error = None;
                        s.restart_count += 1;
                    }
                }
                Err(e) => {
                    warn!(source_id = %id, tap = %tap, error = %e, "Manual sync failed");
                    let mut map = status_map.lock().unwrap();
                    if let Some(s) = map.get_mut(&id) {
                        s.last_error = Some(e.to_string());
                        s.restart_count += 1;
                    }
                }
            }
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Singer subprocess execution
// ---------------------------------------------------------------------------

/// Long-running loop: run tap immediately, then reschedule after poll_interval_secs.
async fn run_tap_loop(
    config: NamedSourceConfig,
    flux_api_url: String,
    status_map: Arc<Mutex<HashMap<String, NamedStatus>>>,
) {
    loop {
        // Record run start time
        {
            let mut map = status_map.lock().unwrap();
            if let Some(s) = map.get_mut(&config.id) {
                s.last_run = Some(Utc::now());
            }
        }
        info!(source_id = %config.id, tap = %config.tap_name, "Singer tap run starting");

        match run_tap_once(&config, &flux_api_url).await {
            Ok(()) => {
                info!(source_id = %config.id, tap = %config.tap_name, "Singer tap run complete");
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config.id) {
                    s.last_error = None;
                    s.restart_count += 1;
                }
            }
            Err(e) => {
                warn!(
                    source_id = %config.id,
                    tap = %config.tap_name,
                    error = %e,
                    "Singer tap run failed"
                );
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config.id) {
                    s.last_error = Some(e.to_string());
                    s.restart_count += 1;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(config.poll_interval_secs)).await;
    }
}

/// Runs one complete tap invocation: discover → spawn → read stdout → wait for exit.
///
/// - Writes config JSON to `/tmp/flux-tap-{id}-config.json` (mode 0600).
/// - Runs `tap --discover` to get a stream catalog; marks all streams selected.
///   Auto-installs the tap via pip if not found on PATH (during discover step).
/// - Writes the selected catalog to `/tmp/flux-tap-{id}-catalog.json`.
/// - If `/tmp/flux-tap-{id}-state.json` exists, passes it via `--state`.
/// - Parses Singer RECORD messages → Flux events → POSTs to flux_api_url.
/// - Persists Singer STATE messages to the state file for incremental sync.
/// - Removes the config and catalog files after the tap exits (state file is kept).
async fn run_tap_once(config: &NamedSourceConfig, flux_api_url: &str) -> Result<()> {
    let config_path = format!("/tmp/flux-tap-{}-config.json", config.id);
    let state_path = format!("/tmp/flux-tap-{}-state.json", config.id);
    let catalog_path = format!("/tmp/flux-tap-{}-catalog.json", config.id);

    // Write tap config with restricted permissions
    tokio::fs::write(&config_path, &config.config_json)
        .await
        .context("Failed to write tap config file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))
            .context("Failed to set permissions on tap config file")?;
    }

    // Run --discover to get a selected catalog; auto-installs tap if missing
    let catalog_json = match run_discover(config, &config_path).await {
        Ok(j) => j,
        Err(e) => {
            let _ = tokio::fs::remove_file(&config_path).await;
            return Err(e);
        }
    };
    tokio::fs::write(&catalog_path, &catalog_json)
        .await
        .context("Failed to write catalog file")?;

    // Build command (tap guaranteed installed after successful discover)
    let mut cmd = tokio::process::Command::new(&config.tap_name);
    cmd.arg("--config").arg(&config_path);
    cmd.arg("--properties").arg(&catalog_path);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    // Attach state file if it exists (incremental sync bookmark)
    if tokio::fs::metadata(&state_path).await.is_ok() {
        cmd.arg("--state").arg(&state_path);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            for path in [&config_path, &catalog_path] {
                let _ = tokio::fs::remove_file(path).await;
            }
            return Err(e.into());
        }
    };

    let stdout = child.stdout.take().expect("stdout is piped");
    let mut lines = BufReader::new(stdout).lines();

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let msg: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                warn!(tap = %config.tap_name, error = %e, "Failed to parse Singer line");
                continue;
            }
        };

        let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "SCHEMA" => {
                // Schema messages are informational — no action needed
            }
            "RECORD" => {
                let singer_stream = msg
                    .get("stream")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let record = match msg.get("record").and_then(|v| v.as_object()) {
                    Some(r) => r.clone(),
                    None => {
                        warn!(tap = %config.tap_name, "RECORD missing record field");
                        continue;
                    }
                };

                // Entity key: configured field, or fallback to first field value
                let key = record
                    .get(&config.entity_key_field)
                    .map(|v| value_to_string(v))
                    .unwrap_or_else(|| {
                        record
                            .values()
                            .next()
                            .map(|v| value_to_string(v))
                            .unwrap_or_else(|| "unknown".to_string())
                    });

                let entity_id = format!("{}/{}", config.namespace, key);

                let safe_tap = config.tap_name.replace('-', ".");
                let safe_stream = singer_stream.replace('-', ".");
                let event = serde_json::json!({
                    "stream": format!("taps.{}.{}", safe_tap, safe_stream),
                    "source": format!("tap.{}", config.tap_name),
                    "timestamp": Utc::now().timestamp_millis(),
                    "key": key,
                    "payload": {
                        "entity_id": entity_id,
                        "properties": record,
                    }
                });

                if let Err(e) = http_client
                    .post(format!("{}/api/events", flux_api_url))
                    .json(&event)
                    .send()
                    .await
                {
                    warn!(tap = %config.tap_name, error = %e, "Failed to post Singer event to Flux");
                }
            }
            "STATE" => {
                // Persist state bookmark for incremental sync on next run
                let state_value =
                    msg.get("value").cloned().unwrap_or(serde_json::Value::Null);
                match serde_json::to_string(&state_value) {
                    Ok(state_json) => {
                        if let Err(e) = tokio::fs::write(&state_path, &state_json).await {
                            warn!(tap = %config.tap_name, error = %e, "Failed to write Singer state file");
                        }
                    }
                    Err(e) => {
                        warn!(tap = %config.tap_name, error = %e, "Failed to serialize Singer state");
                    }
                }
            }
            other => {
                warn!(tap = %config.tap_name, msg_type = other, "Unknown Singer message type — ignoring");
            }
        }
    }

    // Wait for tap to fully exit
    let exit_status = child.wait().await?;
    if !exit_status.success() {
        warn!(
            tap = %config.tap_name,
            code = ?exit_status.code(),
            "Tap exited with non-zero status"
        );
    }

    // Remove config and catalog files; state file is kept for incremental sync
    for path in [&config_path, &catalog_path] {
        if let Err(e) = tokio::fs::remove_file(path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %path, error = %e, "Failed to remove tap temp file");
            }
        }
    }

    Ok(())
}

/// Converts a JSON value to a string for use as a Flux entity key.
fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tap catalog (Phase 3B Task 1)
// ---------------------------------------------------------------------------

/// Raw entry from the Meltano Hub index response.
#[derive(Deserialize)]
struct MeltanoIndexEntry {
    logo_url: Option<String>,
}

/// On-disk cache format.
#[derive(Serialize, Deserialize)]
struct CachedCatalog {
    fetched_at: DateTime<Utc>,
    entries: Vec<TapCatalogEntry>,
}

/// Holds the Meltano Hub tap catalog in memory and manages the on-disk cache.
pub struct TapCatalogStore {
    entries: RwLock<Vec<TapCatalogEntry>>,
    cache_path: String,
}

impl TapCatalogStore {
    /// Create a new store and attempt to load an existing on-disk cache.
    pub fn new(cache_path: &str) -> Self {
        let store = Self {
            entries: RwLock::new(vec![]),
            cache_path: cache_path.to_string(),
        };
        if let Err(e) = store.load_from_disk() {
            warn!(
                cache_path,
                error = %e,
                "Tap catalog cache miss — will fetch from Meltano Hub"
            );
        }
        store
    }

    /// Return all cached catalog entries (may be empty before first refresh).
    pub fn list(&self) -> Vec<TapCatalogEntry> {
        self.entries.read().unwrap().clone()
    }

    /// Returns `true` if the on-disk cache is absent or older than 24 hours.
    pub fn needs_refresh(&self) -> bool {
        if !Path::new(&self.cache_path).exists() {
            return true;
        }
        match std::fs::read_to_string(&self.cache_path)
            .ok()
            .and_then(|s| serde_json::from_str::<CachedCatalog>(&s).ok())
        {
            Some(cached) => {
                Utc::now()
                    .signed_duration_since(cached.fetched_at)
                    .num_seconds()
                    >= CACHE_TTL_SECS
            }
            None => true,
        }
    }

    /// Fetch the extractor index from Meltano Hub, update the in-memory store,
    /// and write the result to the on-disk cache.
    pub async fn refresh(&self) -> Result<usize> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let index: HashMap<String, MeltanoIndexEntry> = client
            .get(MELTANO_INDEX_URL)
            .send()
            .await
            .context("HTTP request to Meltano Hub failed")?
            .error_for_status()
            .context("Meltano Hub returned an error status")?
            .json()
            .await
            .context("Failed to parse Meltano Hub JSON")?;

        let mut entries: Vec<TapCatalogEntry> = index
            .into_iter()
            .map(|(name, entry)| TapCatalogEntry {
                label: derive_label(&name),
                pip_url: name.clone(),
                description: String::new(),
                logo_url: entry.logo_url,
                name,
            })
            .collect();

        entries.sort_by(|a, b| a.name.cmp(&b.name));

        self.save_to_disk(&entries)
            .context("Failed to write tap catalog cache")?;

        let count = entries.len();
        *self.entries.write().unwrap() = entries;
        info!(count, "Tap catalog refreshed from Meltano Hub");
        Ok(count)
    }

    fn load_from_disk(&self) -> Result<()> {
        let data = std::fs::read_to_string(&self.cache_path)
            .context("Cache file not found or unreadable")?;
        let cached: CachedCatalog =
            serde_json::from_str(&data).context("Failed to parse cached catalog JSON")?;
        let count = cached.entries.len();
        *self.entries.write().unwrap() = cached.entries;
        info!(count, cache_path = &self.cache_path, "Tap catalog loaded from cache");
        Ok(())
    }

    fn save_to_disk(&self, entries: &[TapCatalogEntry]) -> Result<()> {
        let cached = CachedCatalog {
            fetched_at: Utc::now(),
            entries: entries.to_vec(),
        };
        let data = serde_json::to_string(&cached)?;
        std::fs::write(&self.cache_path, data)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derives a human-readable label from a Singer tap name.
///
/// Examples:
/// - `"tap-github"` → `"Github"`
/// - `"tap-google-analytics"` → `"Google Analytics"`
fn derive_label(tap_name: &str) -> String {
    let base = tap_name.strip_prefix("tap-").unwrap_or(tap_name);
    base.split('-').map(capitalize).collect::<Vec<_>>().join(" ")
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Singer discover helpers
// ---------------------------------------------------------------------------

/// Runs `tap --discover`, marks all streams selected, returns catalog JSON.
///
/// Auto-installs the tap via pip if the binary is not found on PATH.
async fn run_discover(config: &NamedSourceConfig, config_path: &str) -> Result<String> {
    let result = tokio::process::Command::new(&config.tap_name)
        .arg("--config")
        .arg(config_path)
        .arg("--discover")
        .output()
        .await;

    let output = match result {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!(tap = %config.tap_name, "Tap not found on PATH, attempting pip install");
            let pip = tokio::process::Command::new("pip")
                .arg("install")
                .arg("--break-system-packages")
                .arg(&config.tap_name)
                .status()
                .await;
            match pip {
                Ok(s) if s.success() => {
                    info!(tap = %config.tap_name, "pip install succeeded, retrying --discover");
                    tokio::process::Command::new(&config.tap_name)
                        .arg("--config")
                        .arg(config_path)
                        .arg("--discover")
                        .output()
                        .await
                        .context("Failed to spawn tap after pip install")?
                }
                Ok(s) => {
                    return Err(anyhow::anyhow!(
                        "pip install {} failed (exit code {})",
                        config.tap_name,
                        s.code().unwrap_or(-1)
                    ))
                }
                Err(pe) => {
                    return Err(anyhow::anyhow!(
                        "pip not available ({}); install {} manually",
                        pe, config.tap_name
                    ))
                }
            }
        }
        Err(e) => return Err(e.into()),
    };

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "tap --discover failed (exit code {})",
            output.status.code().unwrap_or(-1)
        ));
    }

    let mut catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse catalog from --discover")?;
    select_all_streams(&mut catalog);
    serde_json::to_string(&catalog).context("Failed to serialize catalog")
}

/// Marks every stream in a Singer catalog as selected.
///
/// Sets `selected: true` at the stream level (older taps) and in the root
/// breadcrumb metadata entry (modern taps), covering both conventions.
fn select_all_streams(catalog: &mut serde_json::Value) {
    let streams = match catalog.get_mut("streams").and_then(|s| s.as_array_mut()) {
        Some(s) => s,
        None => return,
    };
    for stream in streams {
        // Stream-level selection (older taps)
        if let Some(obj) = stream.as_object_mut() {
            obj.insert("selected".to_string(), serde_json::Value::Bool(true));
        }
        // Metadata breadcrumb=[] selection (modern taps)
        if let Some(metadata_arr) = stream.get_mut("metadata").and_then(|m| m.as_array_mut()) {
            let mut found_root = false;
            for entry in metadata_arr.iter_mut() {
                let is_root = entry
                    .get("breadcrumb")
                    .and_then(|b| b.as_array())
                    .map(|b| b.is_empty())
                    .unwrap_or(false);
                if is_root {
                    if let Some(meta) =
                        entry.get_mut("metadata").and_then(|m| m.as_object_mut())
                    {
                        meta.insert("selected".to_string(), serde_json::Value::Bool(true));
                    }
                    found_root = true;
                }
            }
            if !found_root {
                metadata_arr.push(serde_json::json!({
                    "breadcrumb": [],
                    "metadata": { "selected": true }
                }));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    // --- TapCatalogStore tests (Phase 3B Task 1) ---

    #[test]
    fn test_derive_label() {
        assert_eq!(derive_label("tap-github"), "Github");
        assert_eq!(derive_label("tap-google-analytics"), "Google Analytics");
        assert_eq!(derive_label("tap-salesforce"), "Salesforce");
        assert_eq!(derive_label("tap-facebook-ads"), "Facebook Ads");
        assert_eq!(derive_label("something-else"), "Something Else");
    }

    #[test]
    fn test_catalog_store_empty_on_missing_cache() {
        let store = TapCatalogStore::new("/nonexistent/path/catalog.json");
        assert_eq!(store.list().len(), 0);
        assert!(store.needs_refresh());
    }

    #[test]
    fn test_save_and_load_catalog() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let store = TapCatalogStore::new(path);

        let entries = vec![TapCatalogEntry {
            name: "tap-test".to_string(),
            label: "Test".to_string(),
            description: String::new(),
            pip_url: "tap-test".to_string(),
            logo_url: None,
        }];
        store.save_to_disk(&entries).unwrap();
        store.load_from_disk().unwrap();

        let loaded = store.list();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "tap-test");
        assert_eq!(loaded[0].label, "Test");
    }

    #[test]
    fn test_needs_refresh_false_for_fresh_cache() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let store = TapCatalogStore::new(path);
        store.save_to_disk(&[]).unwrap();
        assert!(!store.needs_refresh());
    }

    // --- NamedRunner tests (Phase 3B Task 2) ---

    #[test]
    fn test_value_to_string() {
        assert_eq!(value_to_string(&serde_json::Value::String("abc".to_string())), "abc");
        assert_eq!(value_to_string(&serde_json::json!(42)), "42");
        assert_eq!(value_to_string(&serde_json::json!(3.14)), "3.14");
        assert_eq!(value_to_string(&serde_json::Value::Bool(true)), "true");
        assert_eq!(value_to_string(&serde_json::Value::Null), "null");
    }

    #[test]
    fn test_named_runner_status_empty() {
        use crate::named_config::NamedConfigStore;
        let store = Arc::new(NamedConfigStore::new(":memory:").unwrap());
        let runner = NamedRunner::new(store, "http://localhost:3000".to_string());
        assert!(runner.status().is_empty());
    }
}
