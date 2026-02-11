use anyhow::Result;
use flux::api::{create_router, AppState};
use flux::nats::{EventPublisher, NatsClient, NatsConfig};
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

    // Initialize NATS client
    let nats_config = NatsConfig::default();
    let nats_client = NatsClient::connect(nats_config).await?;
    info!("NATS client connected");

    // Create event publisher
    let event_publisher = EventPublisher::new(nats_client.jetstream().clone());

    // Initialize HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    let app_state = AppState { event_publisher };
    let app = create_router(app_state);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
