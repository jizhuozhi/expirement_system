mod catalog;
mod config;
mod config_source;
mod error;
mod hash;
mod layer;
mod merge;
mod metrics;
mod rule;
mod server;
mod watcher;

#[cfg(feature = "grpc")]
mod grpc_server;

#[cfg(feature = "grpc")]
mod xds_client;

use anyhow::Result;
use config_source::{ConfigChange, ConfigSource, FileSource, GrpcSource};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "grpc")]
use config_source::XdsSource;

#[tokio::main]
async fn main() -> Result<()> {
    // Implementation details
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "experiment_data_plane=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Experiment Data Plane Server");

    // Implementation details
    let config = config::Config::from_env()?;
    tracing::info!("Configuration loaded: {:?}", config);

    // Implementation details
    let source: Arc<dyn ConfigSource> = if config.is_xds_enabled() {
        #[cfg(feature = "grpc")]
        {
            let xds_config = config.xds_config.as_ref().unwrap();
            tracing::info!("Using xDS config source: {}", xds_config.control_plane_addr);
            Arc::new(XdsSource::new(
                xds_config.control_plane_addr.clone(),
                xds_config.node_id.clone(),
                xds_config.cluster.clone(),
                xds_config.services.clone(),
            ))
        }
        #[cfg(not(feature = "grpc"))]
        {
            tracing::error!("xDS configuration requested but gRPC feature not enabled");
            return Err(anyhow::anyhow!("xDS requires gRPC feature"));
        }
    } else if let Ok(grpc_addr) = std::env::var("CONTROL_PLANE_ADDR") {
        tracing::info!("Using legacy gRPC config source: {}", grpc_addr);
        Arc::new(GrpcSource::new(
            grpc_addr,
            std::env::var("DATA_PLANE_ID").unwrap_or_else(|_| "default".to_string()),
            vec![], // Implementation details
        ))
    } else {
        tracing::info!("Using file config source");
        Arc::new(FileSource::new(
            config.layers_dir.clone(),
            config.experiments_dir.clone(),
        ))
    };

    // Implementation details
    tracing::info!("Loading experiment catalog");
    let experiments = source.load_experiments().await?;
    let catalog = Arc::new(RwLock::new(catalog::ExperimentCatalog::from_experiments(
        experiments,
    )?));
    tracing::info!(
        "Experiment catalog loaded: {} experiments",
        catalog.read().await.len()
    );

    // Implementation details
    let layer_manager = Arc::new(layer::LayerManager::new());

    // Implementation details
    let layers = source.load_layers().await?;
    layer_manager
        .load_layers_from_vec(layers, &*catalog.read().await)
        .await?;
    tracing::info!("Initial layers loaded");

    // Implementation details
    let mut change_rx = source.watch_changes().await?;
    let watcher_manager = layer_manager.clone();
    let watcher_catalog = catalog.clone();
    let watcher_handle = tokio::spawn(async move {
        while let Some(change) = change_rx.recv().await {
            if let Err(e) = handle_config_change(&watcher_manager, &watcher_catalog, change).await {
                tracing::error!("Failed to handle config change: {}", e);
            }
        }
        tracing::warn!("Config change receiver closed");
    });

    // Implementation details
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run_server(config, layer_manager, catalog).await {
            tracing::error!("Server error: {}", e);
        }
    });

    // Implementation details
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

async fn handle_config_change(
    manager: &Arc<layer::LayerManager>,
    catalog: &Arc<RwLock<catalog::ExperimentCatalog>>,
    change: ConfigChange,
) -> Result<()> {
    match change {
        ConfigChange::FullReload {
            layers,
            experiments,
        } => {
            tracing::info!(
                "Full config reload: {} layers, {} experiments",
                layers.len(),
                experiments.len()
            );

            // Implementation details
            let new_catalog = catalog::ExperimentCatalog::from_experiments(experiments)?;
            *catalog.write().await = new_catalog;

            // Implementation details
            manager
                .load_layers_from_vec(layers, &*catalog.read().await)
                .await?;
        }
        ConfigChange::LayerUpdate { layer } => {
            tracing::info!("Layer update: {}", layer.layer_id);
            manager.update_layer(layer, &*catalog.read().await).await?;
        }
        ConfigChange::LayerDelete { layer_id } => {
            tracing::info!("Layer delete: {}", layer_id);
            manager
                .remove_layer(&layer_id, &*catalog.read().await)
                .await?;
        }
        ConfigChange::ExperimentUpdate { experiment } => {
            tracing::info!("Experiment update: eid={}", experiment.eid);
            catalog.write().await.update_experiment(experiment)?;

            // Implementation details
            let layers = manager
                .get_layer_ids()
                .into_iter()
                .filter_map(|id| manager.get_layer(&id))
                .map(|arc_layer| (*arc_layer).clone())
                .collect();
            manager
                .load_layers_from_vec(layers, &*catalog.read().await)
                .await?;
        }
        ConfigChange::ExperimentDelete { eid } => {
            tracing::info!("Experiment delete: eid={}", eid);
            catalog.write().await.remove_experiment(eid);
        }
    }

    Ok(())
}
