/// Generic connector runner (Bento subprocess).
/// Phase 3A Task 2: render Bento config, spawn subprocess, monitor status.
use crate::generic_config::{AuthType, GenericConfigStore, GenericSourceConfig};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

/// Runtime status for a single generic source process.
#[derive(Clone, Debug)]
pub struct GenericStatus {
    pub source_id: String,
    pub last_started: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub restart_count: u32,
}

/// Generic connector runner — manages Bento subprocesses for HTTP polling sources.
///
/// Each source runs in a background tokio task that:
/// 1. Writes the rendered YAML config to `/tmp/flux-bento-{id}.yaml`
/// 2. Spawns `bento -c <path>` and waits for it to exit
/// 3. Records an error in status if bento exits with a non-zero code
/// 4. Waits 5 seconds, then repeats (crash recovery loop)
pub struct GenericRunner {
    pub store: Arc<GenericConfigStore>,
    pub flux_api_url: String,
    task_handles: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    status_map: Arc<Mutex<HashMap<String, GenericStatus>>>,
}

impl GenericRunner {
    pub fn new(store: Arc<GenericConfigStore>, flux_api_url: String) -> Self {
        Self {
            store,
            flux_api_url,
            task_handles: Mutex::new(HashMap::new()),
            status_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Starts a background monitoring loop for the given generic source.
    ///
    /// The loop writes the Bento YAML config, spawns `bento -c <path>`, and
    /// restarts it after a 5-second backoff if it crashes. The auth token is
    /// passed as the `FLUX_GENERIC_TOKEN` environment variable — never written
    /// to the config file.
    ///
    /// If `bento` is not found on PATH, the loop logs a warning and exits.
    pub async fn start_source(
        &self,
        config: &GenericSourceConfig,
        token: Option<String>,
    ) -> Result<()> {
        {
            let mut map = self.status_map.lock().unwrap();
            map.entry(config.id.clone()).or_insert_with(|| GenericStatus {
                source_id: config.id.clone(),
                last_started: None,
                last_error: None,
                restart_count: 0,
            });
        }

        let config_owned = config.clone();
        let flux_url = self.flux_api_url.clone();
        let status_map = Arc::clone(&self.status_map);
        let handle = tokio::spawn(run_bento_loop(config_owned, token, flux_url, status_map));

        let mut handles = self.task_handles.lock().unwrap();
        handles.insert(config.id.clone(), handle);
        info!(source_id = %config.id, "Generic source started");
        Ok(())
    }

    /// Aborts the monitoring loop and removes the temp config file.
    ///
    /// No-ops if the source is not running or the config file is already gone.
    pub async fn stop_source(&self, source_id: &str) -> Result<()> {
        let handle = {
            let mut handles = self.task_handles.lock().unwrap();
            handles.remove(source_id)
        };
        if let Some(h) = handle {
            h.abort();
        }

        let config_path = format!("/tmp/flux-bento-{}.yaml", source_id);
        if let Err(e) = tokio::fs::remove_file(&config_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }

        info!(source_id = %source_id, "Generic source stopped");
        Ok(())
    }

    /// Returns current status for all generic sources.
    pub fn status(&self) -> Vec<GenericStatus> {
        let map = self.status_map.lock().unwrap();
        map.values().cloned().collect()
    }
}

/// Long-running loop: write YAML config, spawn bento, wait for exit, restart after 5s backoff.
async fn run_bento_loop(
    config: GenericSourceConfig,
    token: Option<String>,
    flux_api_url: String,
    status_map: Arc<Mutex<HashMap<String, GenericStatus>>>,
) {
    loop {
        let yaml = render_bento_config(&config, &flux_api_url, config.flux_namespace_token.as_deref());
        let config_path = format!("/tmp/flux-bento-{}.yaml", config.id);

        if let Err(e) = tokio::fs::write(&config_path, &yaml).await {
            error!(source_id = %config.id, error = %e, "Failed to write Bento config — retrying in 5s");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            continue;
        }

        let mut cmd = tokio::process::Command::new("bento");
        cmd.arg("-c").arg(&config_path);
        if let Some(ref token_val) = token {
            cmd.env("FLUX_GENERIC_TOKEN", token_val);
        }
        if let Some(ref flux_token) = config.flux_namespace_token {
            cmd.env("FLUX_OUTPUT_TOKEN", flux_token);
        }

        {
            let mut map = status_map.lock().unwrap();
            if let Some(s) = map.get_mut(&config.id) {
                s.last_started = Some(Utc::now());
            }
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(source_id = %config.id, "bento not found on PATH — stopping generic source");
                return;
            }
            Err(e) => {
                error!(source_id = %config.id, error = %e, "Failed to spawn bento — retrying in 5s");
                {
                    let mut map = status_map.lock().unwrap();
                    if let Some(s) = map.get_mut(&config.id) {
                        s.last_error = Some(e.to_string());
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        info!(source_id = %config.id, "Bento subprocess started");

        match child.wait().await {
            Ok(status) if status.success() => {
                info!(source_id = %config.id, "Bento exited cleanly — restarting in 5s");
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config.id) {
                    s.restart_count += 1;
                }
            }
            Ok(status) => {
                let msg = format!("bento exited with code {}", status.code().unwrap_or(-1));
                warn!(source_id = %config.id, %msg, "Bento crashed — restarting in 5s");
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config.id) {
                    s.last_error = Some(msg);
                    s.restart_count += 1;
                }
            }
            Err(e) => {
                let msg = format!("failed to wait for bento: {}", e);
                error!(source_id = %config.id, error = %e, "Error waiting for Bento — restarting in 5s");
                let mut map = status_map.lock().unwrap();
                if let Some(s) = map.get_mut(&config.id) {
                    s.last_error = Some(msg);
                    s.restart_count += 1;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

/// Renders the Bento YAML config for a generic HTTP polling source.
///
/// Source auth token is referenced via `FLUX_GENERIC_TOKEN` env var.
/// Flux output token is referenced via `FLUX_OUTPUT_TOKEN` env var.
/// Neither token is ever embedded in the rendered file.
pub fn render_bento_config(
    config: &GenericSourceConfig,
    flux_api_url: &str,
    flux_namespace_token: Option<&str>,
) -> String {
    let input_headers = match &config.auth_type {
        AuthType::None => String::new(),
        AuthType::BearerToken => {
            "    headers:\n      Authorization: \"Bearer ${FLUX_GENERIC_TOKEN}\"\n".to_string()
        }
        AuthType::ApiKeyHeader { header_name } => {
            format!(
                "    headers:\n      {}: \"${{FLUX_GENERIC_TOKEN}}\"\n",
                header_name
            )
        }
    };

    let output_auth_header = if flux_namespace_token.is_some() {
        "      Authorization: \"Bearer ${FLUX_OUTPUT_TOKEN}\"\n".to_string()
    } else {
        String::new()
    };

    format!(
        r#"http:
  enabled: false

input:
  http_client:
    url: {url}
    verb: GET
{input_headers}    timeout: 30s
    rate_limit: poll_rate

pipeline:
  processors:
    - bloblang: |
        root.stream = "generic"
        root.source = "bento.{source_id}"
        root.timestamp = timestamp_unix_milli()
        root.key = "{entity_key}"
        root.namespace = "{namespace}"
        root.payload.entity_id = "{namespace}/{entity_key}"
        root.payload.properties = this

output:
  http_client:
    url: {flux_api_url}/api/events
    verb: POST
    headers:
      Content-Type: application/json
{output_auth_header}
rate_limit_resources:
  - label: poll_rate
    local:
      count: 1
      interval: {poll_interval_secs}s
"#,
        url = config.url,
        input_headers = input_headers,
        output_auth_header = output_auth_header,
        poll_interval_secs = config.poll_interval_secs,
        source_id = config.id,
        entity_key = config.entity_key,
        namespace = config.namespace,
        flux_api_url = flux_api_url,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_config(auth: AuthType) -> GenericSourceConfig {
        GenericSourceConfig {
            id: "src-001".to_string(),
            name: "Bitcoin Price".to_string(),
            url: "https://api.coingecko.com/api/v3/simple/price".to_string(),
            poll_interval_secs: 300,
            entity_key: "bitcoin".to_string(),
            namespace: "personal".to_string(),
            auth_type: auth,
            created_at: Utc::now(),
            flux_namespace_token: None,
        }
    }

    #[test]
    fn test_render_bento_config_no_auth() {
        let config = make_config(AuthType::None);
        let rendered = render_bento_config(&config, "http://localhost:3000", None);

        assert!(
            rendered.contains("https://api.coingecko.com/api/v3/simple/price"),
            "should contain source URL"
        );
        assert!(rendered.contains("bitcoin"), "should contain entity key");
        assert!(rendered.contains("personal"), "should contain namespace");
        assert!(
            rendered.contains("http://localhost:3000/api/events"),
            "should contain flux output URL"
        );
        assert!(
            !rendered.contains("FLUX_GENERIC_TOKEN"),
            "no_auth must not reference token env var"
        );
        assert!(
            !rendered.contains("FLUX_OUTPUT_TOKEN"),
            "no flux token must not reference output token env var"
        );
    }

    #[test]
    fn test_render_bento_config_bearer_token() {
        let config = make_config(AuthType::BearerToken);
        let rendered = render_bento_config(&config, "http://localhost:3000", None);

        assert!(rendered.contains("https://api.coingecko.com/api/v3/simple/price"));
        assert!(rendered.contains("bitcoin"));
        assert!(rendered.contains("personal"));
        assert!(
            rendered.contains("Bearer ${FLUX_GENERIC_TOKEN}"),
            "bearer must reference token via env var"
        );
        assert!(
            !rendered.contains("actual-secret-token"),
            "must not contain any literal token value"
        );
        assert!(
            !rendered.contains("FLUX_OUTPUT_TOKEN"),
            "no flux token must not add output auth header"
        );
    }

    #[test]
    fn test_render_bento_config_api_key_header() {
        let config = make_config(AuthType::ApiKeyHeader {
            header_name: "X-API-Key".to_string(),
        });
        let rendered = render_bento_config(&config, "http://localhost:3000", None);

        assert!(rendered.contains("https://api.coingecko.com/api/v3/simple/price"));
        assert!(rendered.contains("bitcoin"));
        assert!(rendered.contains("personal"));
        assert!(
            rendered.contains("X-API-Key"),
            "should use custom header name"
        );
        assert!(
            rendered.contains("${FLUX_GENERIC_TOKEN}"),
            "api_key must reference token via env var"
        );
        assert!(
            !rendered.contains("actual-secret-token"),
            "must not contain any literal token value"
        );
    }

    #[test]
    fn test_render_bento_config_with_flux_token() {
        let config = make_config(AuthType::None);
        let rendered =
            render_bento_config(&config, "http://localhost:3000", Some("flux-tok-xyz"));

        assert!(
            rendered.contains("FLUX_OUTPUT_TOKEN"),
            "should reference output token env var"
        );
        assert!(
            rendered.contains("Bearer ${FLUX_OUTPUT_TOKEN}"),
            "should add Authorization header to output section"
        );
        assert!(
            !rendered.contains("flux-tok-xyz"),
            "must not embed literal flux token in config"
        );
    }

    #[test]
    fn test_render_bento_config_bearer_with_flux_token() {
        let config = make_config(AuthType::BearerToken);
        let rendered =
            render_bento_config(&config, "http://localhost:3000", Some("flux-tok-xyz"));

        assert!(
            rendered.contains("Bearer ${FLUX_GENERIC_TOKEN}"),
            "source auth header present"
        );
        assert!(
            rendered.contains("Bearer ${FLUX_OUTPUT_TOKEN}"),
            "flux output auth header present"
        );
    }
}
