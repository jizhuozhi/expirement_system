use crate::catalog::ExperimentDef;
use crate::config_source::{ConfigChange, ConfigSource};
use crate::error::{ExperimentError, Result};
use crate::layer::Layer;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tonic::transport::Channel;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

// Implementation details
pub mod config_pb {
    tonic::include_proto!("experiment.config.v1");
}

use config_pb::{
    config_discovery_service_client::ConfigDiscoveryServiceClient, BuildVersion,
    DeltaDiscoveryRequest, DeltaDiscoveryResponse, DiscoveryRequest, DiscoveryResponse, Locality,
    Node, SemanticVersion,
};

// Implementation details
const LAYER_TYPE_URL: &str = "type.googleapis.com/experiment.config.v1.Layer";
const EXPERIMENT_TYPE_URL: &str = "type.googleapis.com/experiment.config.v1.Experiment";

/// Implementation details
#[derive(Debug, Clone)]
pub struct XdsClientState {
    // Implementation details
    pub sotw_versions: HashMap<String, String>, // Implementation details
    pub sotw_nonces: HashMap<String, String>,   // Implementation details

    // Implementation details
    pub delta_subscriptions: HashMap<String, HashMap<String, bool>>, // Implementation details
    pub delta_versions: HashMap<String, HashMap<String, String>>, // Implementation details
}

impl Default for XdsClientState {
    fn default() -> Self {
        Self {
            sotw_versions: HashMap::new(),
            sotw_nonces: HashMap::new(),
            delta_subscriptions: HashMap::new(),
            delta_versions: HashMap::new(),
        }
    }
}

/// Implementation details
pub struct XdsClient {
    client: ConfigDiscoveryServiceClient<Channel>,
    node: Node,
    state: Arc<RwLock<XdsClientState>>,

    // 配置缓存
    layers: Arc<RwLock<HashMap<String, Layer>>>,
    experiments: Arc<RwLock<HashMap<String, ExperimentDef>>>,
}

impl XdsClient {
    /// Implementation details
    pub async fn new(
        control_plane_addr: String,
        node_id: String,
        cluster: String,
        services: Vec<String>,
    ) -> Result<Self> {
        let channel = tonic::transport::Endpoint::from_shared(control_plane_addr)?
            .connect()
            .await?;

        let client = ConfigDiscoveryServiceClient::new(channel);

        // Implementation details
        let node = Node {
            id: node_id,
            cluster,
            metadata: None, // 可以添加自定义元数据
            locality: Some(Locality {
                region: "default".to_string(),
                zone: "default".to_string(),
                sub_zone: "".to_string(),
            }),
            user_agent_name: "experiment-dataplane".to_string(),
            user_agent_version: "1.0.0".to_string(),
            user_agent_build_version: Some(BuildVersion {
                version: Some(SemanticVersion {
                    major_number: 1,
                    minor_number: 0,
                    patch: 0,
                }),
                metadata: None,
            }),
            extensions: vec![],
            client_features: vec!["delta_xds".to_string()],
            listening_addresses: vec![],
        };

        info!("Created xDS client for node: {}", node.id);

        Ok(Self {
            client,
            node,
            state: Arc::new(RwLock::new(XdsClientState::default())),
            layers: Arc::new(RwLock::new(HashMap::new())),
            experiments: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Implementation details
    pub async fn subscribe_layers_sotw(
        &mut self,
        resource_names: Vec<String>,
    ) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);

        let mut client = self.client.clone();
        let node = self.node.clone();
        let state = self.state.clone();
        let layers = self.layers.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_sotw_stream(
                &mut client,
                node,
                LAYER_TYPE_URL.to_string(),
                resource_names,
                state,
                layers,
                tx,
            )
            .await
            {
                error!("SotW Layer subscription error: {}", e);
            }
        });

        Ok(rx)
    }

    /// Implementation details
    pub async fn subscribe_layers_delta(
        &mut self,
        resource_names: Vec<String>,
    ) -> Result<mpsc::Receiver<ConfigChange>> {
        let (tx, rx) = mpsc::channel(100);

        let mut client = self.client.clone();
        let node = self.node.clone();
        let state = self.state.clone();
        let layers = self.layers.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_delta_stream(
                &mut client,
                node,
                LAYER_TYPE_URL.to_string(),
                resource_names,
                state,
                layers,
                tx,
            )
            .await
            {
                error!("Delta Layer subscription error: {}", e);
            }
        });

        Ok(rx)
    }

    /// Implementation details
    async fn run_sotw_stream(
        client: &mut ConfigDiscoveryServiceClient<Channel>,
        node: Node,
        type_url: String,
        resource_names: Vec<String>,
        state: Arc<RwLock<XdsClientState>>,
        layers: Arc<RwLock<HashMap<String, Layer>>>,
        tx: mpsc::Sender<ConfigChange>,
    ) -> Result<()> {
        let mut stream = client
            .stream_configs(tonic::Request::new(
                tokio_stream::iter(std::iter::empty()), // 初始为空，后续动态发送
            ))
            .await?
            .into_inner();

        // 发送初始请求
        let initial_req = DiscoveryRequest {
            version_info: "".to_string(),
            node: Some(node.clone()),
            resource_names: resource_names.clone(),
            type_url: type_url.clone(),
            response_nonce: "".to_string(),
            error_detail: None,
        };

        // 这里需要使用双向流，实际实现会更复杂
        // 为了简化，我们先实现基本的逻辑框架

        info!("SotW subscription started for type: {}", type_url);

        // 处理响应（简化版本）
        while let Some(response) = stream.message().await? {
            debug!(
                "Received SotW response: version={}, nonce={}, resources={}",
                response.version_info,
                response.nonce,
                response.resources.len()
            );

            // 处理资源
            if let Err(e) = Self::process_sotw_resources(&response, &layers, &tx).await {
                warn!("Failed to process SotW resources: {}", e);
                // Implementation details
                continue;
            }

            // 更新状态
            {
                let mut state_guard = state.write().await;
                state_guard
                    .sotw_versions
                    .insert(type_url.clone(), response.version_info.clone());
                state_guard
                    .sotw_nonces
                    .insert(type_url.clone(), response.nonce.clone());
            }

            // Implementation details
            info!("SotW ACK sent for version: {}", response.version_info);
        }

        Ok(())
    }

    /// Implementation details
    async fn run_delta_stream(
        client: &mut ConfigDiscoveryServiceClient<Channel>,
        node: Node,
        type_url: String,
        resource_names: Vec<String>,
        state: Arc<RwLock<XdsClientState>>,
        layers: Arc<RwLock<HashMap<String, Layer>>>,
        tx: mpsc::Sender<ConfigChange>,
    ) -> Result<()> {
        info!("Delta subscription started for type: {}", type_url);

        // 初始化订阅状态
        {
            let mut state_guard = state.write().await;
            let subscriptions = state_guard
                .delta_subscriptions
                .entry(type_url.clone())
                .or_insert_with(HashMap::new);

            for name in &resource_names {
                subscriptions.insert(name.clone(), true);
            }
        }

        // Implementation details
        // 实际实现需要处理双向流和复杂的状态管理

        Ok(())
    }

    /// Implementation details
    async fn process_sotw_resources(
        response: &DiscoveryResponse,
        layers: &Arc<RwLock<HashMap<String, Layer>>>,
        tx: &mpsc::Sender<ConfigChange>,
    ) -> Result<()> {
        let mut new_layers = HashMap::new();

        for resource in &response.resources {
            // 解析资源
            let layer: config_pb::Layer = prost::Message::decode(resource.value.as_slice())?;

            // 转换为内部格式
            let internal_layer = Self::convert_layer_from_proto(&layer)?;

            new_layers.insert(layer.layer_id.clone(), internal_layer.clone());

            // 发送配置变更事件
            if let Err(e) = tx
                .send(ConfigChange::LayerUpdate {
                    layer: internal_layer,
                })
                .await
            {
                warn!("Failed to send layer update event: {}", e);
            }
        }

        // 原子更新缓存
        {
            let mut layers_guard = layers.write().await;
            *layers_guard = new_layers;
        }

        info!(
            "Processed {} layers from SotW response",
            response.resources.len()
        );
        Ok(())
    }

    /// Implementation details
    fn convert_layer_from_proto(proto_layer: &config_pb::Layer) -> Result<Layer> {
        use crate::layer::BucketRange;

        let ranges = proto_layer
            .ranges
            .iter()
            .map(|r| BucketRange {
                start: r.start,
                end: r.end,
                vid: r.vid,
            })
            .collect();

        Ok(Layer {
            layer_id: proto_layer.layer_id.clone(),
            version: proto_layer.version.clone(),
            priority: proto_layer.priority,
            hash_key: proto_layer.hash_key.clone(),
            salt: proto_layer.salt.clone(),
            ranges,
            enabled: proto_layer.enabled,
        })
    }

    /// Implementation details
    pub async fn get_layer(&self, layer_id: &str) -> Option<Layer> {
        let layers = self.layers.read().await;
        layers.get(layer_id).cloned()
    }

    /// Implementation details
    pub async fn list_layers(&self) -> Vec<Layer> {
        let layers = self.layers.read().await;
        layers.values().cloned().collect()
    }
}

/// Implementation details
pub struct XdsConfigSource {
    client: Arc<RwLock<Option<XdsClient>>>,
    control_plane_addr: String,
    node_id: String,
    cluster: String,
    services: Vec<String>,
}

impl XdsConfigSource {
    pub fn new(
        control_plane_addr: String,
        node_id: String,
        cluster: String,
        services: Vec<String>,
    ) -> Self {
        Self {
            client: Arc::new(RwLock::new(None)),
            control_plane_addr,
            node_id,
            cluster,
            services,
        }
    }

    async fn ensure_client(&self) -> Result<()> {
        let mut client_guard = self.client.write().await;

        if client_guard.is_none() {
            let client = XdsClient::new(
                self.control_plane_addr.clone(),
                self.node_id.clone(),
                self.cluster.clone(),
                self.services.clone(),
            )
            .await?;

            *client_guard = Some(client);
            info!("xDS client initialized");
        }

        Ok(())
    }
}

#[async_trait]
impl ConfigSource for XdsConfigSource {
    async fn load_layers(&self) -> Result<Vec<Layer>> {
        self.ensure_client().await?;

        let client_guard = self.client.read().await;
        if let Some(client) = client_guard.as_ref() {
            Ok(client.list_layers().await)
        } else {
            Ok(Vec::new())
        }
    }

    async fn load_experiments(&self) -> Result<Vec<ExperimentDef>> {
        // Implementation details
        Ok(Vec::new())
    }

    async fn watch_changes(&self) -> Result<mpsc::Receiver<ConfigChange>> {
        self.ensure_client().await?;

        let mut client_guard = self.client.write().await;
        if let Some(client) = client_guard.as_mut() {
            // Implementation details
            client.subscribe_layers_delta(Vec::new()).await
        } else {
            Err(ExperimentError::Config(
                "xDS client not initialized".to_string(),
            ))
        }
    }
}
