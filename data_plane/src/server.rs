use crate::catalog::ExperimentCatalog;
use crate::config::Config;
use crate::layer::LayerManager;
use crate::merge::{merge_layers_batch, ExperimentRequest, ExperimentResponse};
use crate::metrics;
use crate::rule::FieldType;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::RwLock;
use prometheus::{Encoder, TextEncoder};
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
struct AppState {
    layer_manager: Arc<LayerManager>,
    catalog: Arc<ExperimentCatalog>,
    field_types: Arc<RwLock<HashMap<String, FieldType>>>,
}

pub async fn run_server(
    config: Config,
    layer_manager: Arc<LayerManager>,
    catalog: Arc<ExperimentCatalog>,
) -> anyhow::Result<()> {
    // Initialize metrics
    metrics::init();

    let state = AppState {
        layer_manager,
        catalog,
        field_types: Arc::new(RwLock::new(HashMap::new())),
    };

    // Build application router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/experiment", post(experiment_handler))
        .route("/layers", get(list_layers))
        .route("/layers/:layer_id", get(get_layer))
        .route("/layers/:layer_id/rollback", post(rollback_layer))
        .route("/field_types", get(get_field_types))
        .route("/field_types", post(update_field_types))
        .route("/metrics", get(metrics_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("{}:{}", config.server_host, config.server_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "experiment-data-plane"
    }))
}

async fn experiment_handler(
    State(state): State<AppState>,
    Json(request): Json<ExperimentRequest>,
) -> Result<Json<ExperimentResponse>, AppError> {
    let _timer = metrics::REQUEST_DURATION.start_timer();
    metrics::REQUEST_TOTAL.inc();

    // Get field types
    let field_types = state.field_types.read().clone();

    // Merge layers with rule evaluation using batch API
    let response =
        merge_layers_batch(&request, &state.layer_manager, &state.catalog, &field_types).map_err(
            |e| {
                metrics::REQUEST_ERRORS.inc();
                e
            },
        )?;

    // Update active layers metric
    let total_layers: usize = response
        .results
        .values()
        .map(|r| r.matched_layers.len())
        .sum();
    metrics::ACTIVE_LAYERS.set(total_layers as i64);

    Ok(Json(response))
}

async fn list_layers(State(state): State<AppState>) -> impl IntoResponse {
    let layer_ids = state.layer_manager.get_layer_ids();
    Json(serde_json::json!({
        "layers": layer_ids
    }))
}

async fn get_layer(
    State(state): State<AppState>,
    Path(layer_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let layer = state
        .layer_manager
        .get_layer(&layer_id)
        .ok_or_else(|| crate::error::ExperimentError::LayerNotFound(layer_id.clone()))?;

    Ok(Json(serde_json::to_value(&*layer)?))
}

async fn rollback_layer(
    State(state): State<AppState>,
    Path(layer_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.layer_manager.rollback_layer(&layer_id).await?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": format!("Layer {} rolled back", layer_id)
    })))
}

async fn get_field_types(State(state): State<AppState>) -> impl IntoResponse {
    let field_types = state.field_types.read().clone();
    Json(field_types)
}

async fn update_field_types(
    State(state): State<AppState>,
    Json(new_field_types): Json<HashMap<String, FieldType>>,
) -> impl IntoResponse {
    let mut field_types = state.field_types.write();
    *field_types = new_field_types;

    tracing::info!("Updated field types: {} fields", field_types.len());

    Json(serde_json::json!({
        "status": "success",
        "message": format!("Updated {} field types", field_types.len())
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

// Error handling
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let message = self.0.to_string();
        tracing::error!("Request error: {}", message);

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": message
            })),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
