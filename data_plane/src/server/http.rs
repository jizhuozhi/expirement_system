use crate::catalog::ExperimentCatalog;
use crate::config::Config;
use crate::layer::LayerManager;
use crate::metrics;
use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use parking_lot::RwLock;
use prometheus::{Encoder, TextEncoder};
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;
use tower_http::trace::TraceLayer;

#[cfg(feature = "grpc")]
use crate::grpc_server;

/// Implementation details
/// Implementation details
/// Implementation details
///
/// Implementation details
pub async fn run_server(
    config: Config,
    layer_manager: Arc<LayerManager>,
    catalog: Arc<TokioRwLock<ExperimentCatalog>>,
) -> anyhow::Result<()> {
    // Implementation details
    metrics::init();

    // Implementation details
    let field_types = Arc::new(RwLock::new(config.load_field_types()?));

    // Implementation details
    #[cfg(feature = "grpc")]
    {
        let grpc_addr = format!(
            "{}:{}",
            config.server_host,
            config.grpc_port.unwrap_or(50051)
        );
        let grpc_layer_manager = layer_manager.clone();
        let grpc_catalog = catalog.clone();
        let grpc_field_types = field_types.clone();

        tokio::spawn(async move {
            if let Err(e) = grpc_server::run_grpc_server(
                grpc_addr,
                grpc_layer_manager,
                grpc_catalog,
                grpc_field_types,
            )
            .await
            {
                tracing::error!("gRPC server error: {}", e);
            }
        });
    }

    // Implementation details
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .layer(TraceLayer::new_for_http());

    let addr = format!("{}:{}", config.server_host, config.server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("HTTP server (health/metrics) listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "experiment-data-plane"
    }))
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = metrics::REGISTRY.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        buffer,
    )
}
