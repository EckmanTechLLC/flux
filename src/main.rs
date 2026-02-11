use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "flux=info".into()),
        )
        .init();

    info!("Flux starting...");

    // TODO: Initialize components (Task 3-6)
    // - NATS client
    // - State engine
    // - HTTP/WebSocket server

    Ok(())
}
