use anyhow::Result;
use axum::{routing::get, Router};
use flux::api::{
    create_deletion_router, create_namespace_router, create_query_router, create_router,
    ws_handler, AppState, DeletionAppState, QueryAppState, WsAppState,
};
use flux::config;
use flux::namespace::NamespaceRegistry;
use flux::nats::{EventPublisher, NatsClient};
use flux::snapshot::{manager::SnapshotManager, recovery};
use flux::state::StateEngine;
use std::path::PathBuf;
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

    // Load configuration
    let config_path = std::env::var("FLUX_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let flux_config = config::load_config(&config_path).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "Failed to load config, using defaults");
        config::FluxConfig::default()
    });

    // Initialize NATS client
    let nats_config = flux_config.nats.clone();
    let nats_client = NatsClient::connect(nats_config).await?;
    info!("NATS client connected");

    // Create event publisher
    let event_publisher = EventPublisher::new(nats_client.jetstream().clone());

    // Create state engine
    let state_engine = Arc::new(StateEngine::new());
    info!("State engine initialized");

    // Recovery: Try to load latest snapshot
    let snapshot_dir = PathBuf::from(&flux_config.snapshot.directory);
    let start_sequence = match recovery::load_latest_snapshot(&snapshot_dir)? {
        Some((snapshot, seq)) => {
            info!(
                sequence = seq,
                entities = snapshot.entity_count(),
                "Loaded snapshot: seq={}, entities={}",
                seq,
                snapshot.entity_count()
            );
            state_engine.load_from_snapshot(snapshot.to_hashmap(), seq);
            Some(seq)
        }
        None => {
            info!("No snapshot found, starting from beginning");
            None
        }
    };

    // Start state engine subscriber (background task)
    let engine_clone = Arc::clone(&state_engine);
    let jetstream_clone = nats_client.jetstream().clone();
    tokio::spawn(async move {
        if let Err(e) = engine_clone.run_subscriber(jetstream_clone, start_sequence).await {
            tracing::error!(error = %e, "State engine subscriber failed");
        }
    });
    info!("State engine subscriber started");

    // Start metrics broadcaster (background task)
    let engine_clone = Arc::clone(&state_engine);
    let metrics_config = flux_config.metrics.clone();
    tokio::spawn(async move {
        flux::state::run_metrics_broadcaster(
            engine_clone,
            metrics_config.broadcast_interval_seconds,
            metrics_config.active_publisher_window_seconds,
        )
        .await;
    });
    info!("Metrics broadcaster started");

    // Start snapshot manager (background task)
    let snapshot_manager = SnapshotManager::new(Arc::clone(&state_engine), flux_config.snapshot.clone());
    tokio::spawn(async move {
        if let Err(e) = snapshot_manager.run_snapshot_loop().await {
            tracing::error!(error = %e, "Snapshot manager failed");
        }
    });
    info!("Snapshot manager started");

    // Initialize HTTP server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    // Auth configuration (default: disabled for backward compatibility)
    let auth_enabled = std::env::var("FLUX_AUTH_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    info!("Auth enabled: {}", auth_enabled);

    // Create namespace registry (for auth mode)
    let namespace_registry = Arc::new(NamespaceRegistry::new());

    // Create ingestion API router
    let ingestion_state = AppState {
        event_publisher: event_publisher.clone(),
        namespace_registry: Arc::clone(&namespace_registry),
        auth_enabled,
    };
    let ingestion_router = create_router(ingestion_state.clone());

    // Create namespace API router (reuses ingestion_state)
    let namespace_router = create_namespace_router(ingestion_state);

    // Create deletion API router
    let deletion_state = DeletionAppState {
        event_publisher: event_publisher.clone(),
        namespace_registry: Arc::clone(&namespace_registry),
        state_engine: Arc::clone(&state_engine),
        auth_enabled,
        max_batch_delete: flux_config.api.max_batch_delete,
    };
    let deletion_router = create_deletion_router(deletion_state);

    // Create WebSocket API router
    let ws_state = Arc::new(WsAppState {
        state_engine: Arc::clone(&state_engine),
    });
    let ws_router = Router::new()
        .route("/api/ws", get(ws_handler))
        .with_state(ws_state);

    // Create Query API router
    let query_state = Arc::new(QueryAppState { state_engine });
    let query_router = create_query_router(query_state);

    // Combine routers
    let app = ingestion_router
        .merge(namespace_router)
        .merge(deletion_router)
        .merge(ws_router)
        .merge(query_router);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
