mod config;
mod error;
mod layer;
mod merge;
mod hash;
mod rule;
mod server;
mod watcher;
mod metrics;

use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "experiment_data_plane=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Experiment Data Plane Server");

    // Load configuration
    let config = config::Config::from_env()?;
    tracing::info!("Configuration loaded: {:?}", config);

    // Initialize layer manager
    let layer_manager = Arc::new(layer::LayerManager::new(config.layers_dir.clone()));

    // Load initial layers
    layer_manager.load_all_layers().await?;
    tracing::info!("Initial layers loaded");

    // Start file watcher for hot reload
    let watcher_manager = layer_manager.clone();
    let watcher_handle = tokio::spawn(async move {
        if let Err(e) = watcher::watch_layers(watcher_manager).await {
            tracing::error!("Watcher error: {}", e);
        }
    });

    // Start HTTP server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run_server(config, layer_manager).await {
            tracing::error!("Server error: {}", e);
        }
    });

    // Wait for both tasks
    tokio::select! {
        _ = watcher_handle => {
            tracing::warn!("Watcher stopped");
        }
        _ = server_handle => {
            tracing::warn!("Server stopped");
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal");
        }
    }

    Ok(())
}
