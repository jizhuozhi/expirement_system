use crate::catalog::ExperimentCatalog;
use crate::layer::LayerManager;
use crate::merge::{merge_layers_batch, ExperimentRequest};
use crate::metrics;
use crate::rule::FieldType;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

// Implementation details
pub struct ExperimentServiceImpl {
    layer_manager: Arc<LayerManager>,
    catalog: Arc<TokioRwLock<ExperimentCatalog>>,
    field_types: Arc<RwLock<HashMap<String, FieldType>>>,
}

impl ExperimentServiceImpl {
    pub fn new(
        layer_manager: Arc<LayerManager>,
        catalog: Arc<TokioRwLock<ExperimentCatalog>>,
        field_types: Arc<RwLock<HashMap<String, FieldType>>>,
    ) -> Self {
        Self {
            layer_manager,
            catalog,
            field_types,
        }
    }

    pub async fn get_experiment(
        &self,
        request: ExperimentRequest,
    ) -> anyhow::Result<serde_json::Value> {
        let _timer = metrics::REQUEST_DURATION.start_timer();
        metrics::REQUEST_TOTAL.inc();

        // Implementation details
        let field_types = self.field_types.read().clone();

        // Implementation details
        let internal_resp = merge_layers_batch(
            &request,
            &self.layer_manager,
            &*self.catalog.read().await,
            &field_types,
        )
        .map_err(|e| {
            metrics::REQUEST_ERRORS.inc();
            anyhow::anyhow!(e.to_string())
        })?;

        // Implementation details
        let total_layers: usize = internal_resp
            .results
            .values()
            .map(|r| r.matched_layers.len())
            .sum();
        metrics::ACTIVE_LAYERS.set(total_layers as i64);

        // Implementation details
        Ok(serde_json::to_value(&internal_resp)?)
    }
}

pub async fn run_grpc_server(
    addr: String,
    layer_manager: Arc<LayerManager>,
    catalog: Arc<TokioRwLock<ExperimentCatalog>>,
    field_types: Arc<RwLock<HashMap<String, FieldType>>>,
) -> anyhow::Result<()> {
    let _service = ExperimentServiceImpl::new(layer_manager, catalog, field_types);

    tracing::info!("gRPC server would listen on {} (proto disabled)", addr);
    
    // Implementation details
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    Ok(())
}
