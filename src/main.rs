use anyhow::Result;
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use flux::api::{
    create_admin_router, create_connector_router, create_deletion_router, create_history_router,
    create_namespace_router, create_oauth_router, create_query_router, create_router,
    create_ws_router, run_state_cleanup, AdminAppState, AppState, ConnectorAppState,
    DeletionAppState, HistoryAppState, OAuthAppState, QueryAppState, StateManager, WsAppState,
};
use flux::rate_limit::RateLimiter;
use flux::config;
use flux::config::new_runtime_config;
use flux::credentials::CredentialStore;
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

    // Initialize runtime config (loaded from env vars, defaults otherwise)
    let runtime_config = new_runtime_config();
    info!("Runtime config initialized");

    // Admin token (for PUT /api/admin/config)
    let admin_token = std::env::var("FLUX_ADMIN_TOKEN").ok();
    if admin_token.is_none() {
        tracing::warn!("FLUX_ADMIN_TOKEN not set - admin config PUT is unrestricted");
    }

    // Auth configuration (default: disabled for backward compatibility)
    let auth_enabled = std::env::var("FLUX_AUTH_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    info!("Auth enabled: {}", auth_enabled);

    // Create namespace registry (for auth mode)
    let namespace_registry = Arc::new(NamespaceRegistry::new());

    // Initialize credential store (for connector framework)
    let credential_store = std::env::var("FLUX_ENCRYPTION_KEY")
        .ok()
        .and_then(|key| {
            let db_path = std::env::var("FLUX_CREDENTIALS_DB")
                .unwrap_or_else(|_| "credentials.db".to_string());

            match CredentialStore::new(&db_path, &key) {
                Ok(store) => {
                    info!("Credential store initialized at {}", db_path);
                    Some(Arc::new(store))
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to initialize credential store (connectors disabled)"
                    );
                    None
                }
            }
        });

    if credential_store.is_none() {
        tracing::warn!("FLUX_ENCRYPTION_KEY not set - connector framework disabled");
    }

    // Initialize rate limiter (per-namespace token buckets, auth-gated)
    let rate_limiter = Arc::new(RateLimiter::new());
    info!("Rate limiter initialized");

    // Create ingestion API router
    let ingestion_state = AppState {
        event_publisher: event_publisher.clone(),
        namespace_registry: Arc::clone(&namespace_registry),
        auth_enabled,
        admin_token: admin_token.clone(),
        runtime_config: Arc::clone(&runtime_config),
        rate_limiter,
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

    // Create WebSocket API router (no auth — WS is read-only)
    let ws_state = Arc::new(WsAppState {
        state_engine: Arc::clone(&state_engine),
    });
    let ws_router = create_ws_router(ws_state);

    // Create Query API router
    let query_state = Arc::new(QueryAppState { state_engine });
    let query_router = create_query_router(query_state);

    // Create History API router
    let history_state = Arc::new(HistoryAppState {
        jetstream: nats_client.jetstream().clone(),
    });
    let history_router = create_history_router(history_state);

    // Create Connector API router
    let connector_state = ConnectorAppState {
        credential_store: credential_store.clone(),
        namespace_registry: Arc::clone(&namespace_registry),
        auth_enabled,
    };
    let connector_router = create_connector_router(connector_state);

    // Create OAuth API router (requires credential store)
    let oauth_router = if let Some(ref store) = credential_store {
        // Create OAuth state manager
        let state_manager = StateManager::new(600); // 10 minutes expiry

        // Start state cleanup background task
        let cleanup_manager = state_manager.clone();
        tokio::spawn(async move {
            run_state_cleanup(cleanup_manager, 300).await; // Cleanup every 5 minutes
        });
        info!("OAuth state manager started");

        // Get callback base URL from environment
        let callback_base_url = std::env::var("FLUX_OAUTH_CALLBACK_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        info!("OAuth callback base URL: {}", callback_base_url);

        let oauth_state = OAuthAppState {
            credential_store: Arc::clone(store),
            namespace_registry: Arc::clone(&namespace_registry),
            state_manager,
            auth_enabled,
            callback_base_url,
        };

        create_oauth_router(oauth_state)
    } else {
        // OAuth disabled without credential store
        Router::new()
    };

    // Create Admin API router
    let admin_state = AdminAppState {
        runtime_config,
        admin_token,
    };
    let admin_router = create_admin_router(admin_state);

    // CORS — allow browsers (flux-universe.com explorer) to fetch from Flux
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ]);

    // Combine routers
    let app = ingestion_router
        .merge(namespace_router)
        .merge(deletion_router)
        .merge(ws_router)
        .merge(query_router)
        .merge(history_router)
        .merge(connector_router)
        .merge(oauth_router)
        .merge(admin_router)
        .layer(cors);

    let addr = format!("0.0.0.0:{}", port);
    info!("Starting HTTP server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
