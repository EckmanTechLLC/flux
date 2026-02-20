use anyhow::{Context, Result};
use connector_manager::manager::ConnectorManager;
use flux::credentials::CredentialStore;
use std::sync::Arc;
use tracing::info;

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

    info!(
        flux_api_url = %flux_api_url,
        credentials_db = %credentials_db,
        "Configuration loaded"
    );

    // Initialize credential store
    let credential_store = CredentialStore::new(&credentials_db, &encryption_key)
        .context("Failed to initialize credential store")?;
    let credential_store = Arc::new(credential_store);
    info!("Credential store initialized");

    // Initialize connector manager
    let mut manager = ConnectorManager::new(credential_store, flux_api_url);

    // Start manager (logs available connectors; schedulers start on-demand via OAuth)
    let started = manager.start().await?;
    info!(schedulers_started = started, "Connector manager started");

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .context("Failed to listen for ctrl_c signal")?;

    info!("Shutdown signal received");

    // Graceful shutdown
    manager.shutdown().await;
    info!("Connector manager stopped");

    Ok(())
}
