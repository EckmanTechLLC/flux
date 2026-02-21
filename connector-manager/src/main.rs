use anyhow::{Context, Result};
use connector_manager::api::{create_router, ApiState};
use connector_manager::generic_config::GenericConfigStore;
use connector_manager::manager::ConnectorManager;
use connector_manager::named_config::NamedConfigStore;
use connector_manager::runners::generic::GenericRunner;
use connector_manager::runners::named::{NamedRunner, TapCatalogStore};
use flux::credentials::CredentialStore;
use std::sync::Arc;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "connector_manager=info".into()),
        )
        .init();

    info!("Connector Manager starting...");

    // Read configuration from environment
    let flux_api_url = std::env::var("FLUX_API_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let encryption_key = std::env::var("FLUX_ENCRYPTION_KEY")
        .context("FLUX_ENCRYPTION_KEY is required (base64-encoded 32-byte key)")?;

    let credentials_db = std::env::var("FLUX_CREDENTIALS_DB")
        .unwrap_or_else(|_| "credentials.db".to_string());

    let generic_config_db = std::env::var("GENERIC_CONFIG_DB")
        .unwrap_or_else(|_| "generic_config.db".to_string());

    let named_config_db = std::env::var("NAMED_CONFIG_DB")
        .unwrap_or_else(|_| "named_config.db".to_string());

    let api_port: u16 = std::env::var("CONNECTOR_API_PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse()
        .context("CONNECTOR_API_PORT must be a valid port number")?;

    info!(
        flux_api_url = %flux_api_url,
        credentials_db = %credentials_db,
        generic_config_db = %generic_config_db,
        named_config_db = %named_config_db,
        api_port = api_port,
        "Configuration loaded"
    );

    // Initialize credential store (shared by manager and generic runner)
    let credential_store = Arc::new(
        CredentialStore::new(&credentials_db, &encryption_key)
            .context("Failed to initialize credential store")?,
    );
    info!("Credential store initialized");

    // Initialize generic config store
    let generic_config_store = Arc::new(
        GenericConfigStore::new(&generic_config_db)
            .context("Failed to initialize generic config store")?,
    );
    info!("Generic config store initialized");

    // Initialize generic runner
    let generic_runner = Arc::new(GenericRunner::new(
        Arc::clone(&generic_config_store),
        flux_api_url.clone(),
    ));

    // Restart any persisted generic sources from a previous session
    let persisted = generic_config_store
        .list()
        .context("Failed to list persisted generic sources")?;
    if !persisted.is_empty() {
        info!(count = persisted.len(), "Restarting persisted generic sources");
        for config in &persisted {
            let token = credential_store
                .get("generic", &config.id)
                .ok()
                .flatten()
                .map(|c| c.access_token);
            if let Err(e) = generic_runner.start_source(config, token).await {
                warn!(source_id = %config.id, error = %e, "Failed to restart generic source");
            }
        }
    }

    // Initialize named config store
    let named_config_store = Arc::new(
        NamedConfigStore::new(&named_config_db)
            .context("Failed to initialize named config store")?,
    );
    info!("Named config store initialized");

    // Initialize named runner
    let named_runner = Arc::new(NamedRunner::new(
        Arc::clone(&named_config_store),
        flux_api_url.clone(),
    ));

    // Restart any persisted named sources from a previous session
    let persisted_named = named_config_store
        .list()
        .context("Failed to list persisted named sources")?;
    if !persisted_named.is_empty() {
        info!(count = persisted_named.len(), "Restarting persisted named sources");
        for config in &persisted_named {
            if let Err(e) = named_runner.start_source(config).await {
                warn!(source_id = %config.id, tap = %config.tap_name, error = %e, "Failed to restart named source");
            }
        }
    }

    // Initialize tap catalog store (load from disk if cached, else empty)
    let tap_catalog_path = std::env::var("TAP_CATALOG_CACHE")
        .unwrap_or_else(|_| "/tmp/flux-tap-catalog.json".to_string());
    let tap_catalog = Arc::new(TapCatalogStore::new(&tap_catalog_path));
    info!(cache_path = %tap_catalog_path, "Tap catalog store initialized");

    // Background task: refresh catalog from Meltano Hub if stale
    let catalog_for_bg = Arc::clone(&tap_catalog);
    tokio::spawn(async move {
        if catalog_for_bg.needs_refresh() {
            match catalog_for_bg.refresh().await {
                Ok(count) => info!(count, "Tap catalog loaded from Meltano Hub"),
                Err(e) => warn!(error = %e, "Tap catalog fetch failed — catalog will be empty"),
            }
        }
    });

    // Initialize connector manager (builtin connectors)
    let mut manager = ConnectorManager::new(Arc::clone(&credential_store), flux_api_url);
    let started = manager.start().await?;
    info!(schedulers_started = started, "Connector manager started");

    // Start HTTP API server
    let api_state = ApiState {
        config_store: Arc::clone(&generic_config_store),
        runner: Arc::clone(&generic_runner),
        credential_store: Arc::clone(&credential_store),
        tap_catalog: Arc::clone(&tap_catalog),
        named_runner: Arc::clone(&named_runner),
    };
    let router = create_router(api_state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", api_port))
        .await
        .context("Failed to bind connector API port")?;
    info!(port = api_port, "Connector API listening");

    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "Connector API server error");
        }
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .context("Failed to listen for ctrl_c signal")?;
    info!("Shutdown signal received");

    // Graceful shutdown
    server_handle.abort();
    manager.shutdown().await;
    info!("Connector manager stopped");

    Ok(())
}
