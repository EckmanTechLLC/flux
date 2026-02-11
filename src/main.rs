use anyhow::Result;
use axum::{routing::get, Router};
use flux::api::{create_router, ws_handler, AppState, WsAppState};
use flux::nats::{EventPublisher, NatsClient, NatsConfig};
use flux::state::StateEngine;
use std::sync::Arc;
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

    // Create state engine
    let state_engine = Arc::new(StateEngine::new());
    info!("State engine initialized");

    // Start state engine subscriber (background task)
    let engine_clone = Arc::clone(&state_engine);
    let jetstream_clone = nats_client.jetstream().clone();
    tokio::spawn(async move {
        if let Err(e) = engine_clone.run_subscriber(jetstream_clone).await {
            tracing::error!(error = %e, "State engine subscriber failed");
        }
    });
    info!("State engine subscriber started");

    // Initialize HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    // Create ingestion API router
    let ingestion_state = AppState { event_publisher };
    let ingestion_router = create_router(ingestion_state);

    // Create WebSocket API router
    let ws_state = Arc::new(WsAppState { state_engine });
    let ws_router = Router::new()
        .route("/api/ws", get(ws_handler))
        .with_state(ws_state);

    // Combine routers
    let app = ingestion_router.merge(ws_router);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
