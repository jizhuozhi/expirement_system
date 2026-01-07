use crate::error::{ExperimentError, Result};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Bucket size (10000 slots = 0.01% granularity)
pub const BUCKET_SIZE: u32 = 10000;

/// Layer definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub layer_id: String,
    pub version: String,
    pub priority: i32,
    pub hash_key: String,
    
    /// Optional salt for hash calculation
    /// If not provided, defaults to "{layer_id}_{version}"
    #[serde(default)]
    pub salt: Option<String>,
    
    /// Bucket index -> Group ID mapping
    pub buckets: HashMap<u32, String>,
    
    /// Group ID -> Group definition mapping
    pub groups: HashMap<String, Group>,
    
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub service: String,
    
    /// JSON or YAML formatted parameters
    pub params: serde_json::Value,
    
    /// Optional rule for this group
    #[serde(default)]
    pub rule: Option<crate::rule::Node>,
}

impl Layer {
    /// Get the salt for this layer
    /// If salt is not configured, use "{layer_id}_{version}" as default
    pub fn get_salt(&self) -> String {
        self.salt
            .clone()
            .unwrap_or_else(|| format!("{}_{}", self.layer_id, self.version))
    }
    
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        
        // Try JSON first, then YAML
        let layer: Layer = serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content).map_err(ExperimentError::from))?;
        
        // Validate layer
        layer.validate()?;
        
        Ok(layer)
    }
    
    pub fn validate(&self) -> Result<()> {
        // Validate buckets are within range
        for &bucket_id in self.buckets.keys() {
            if bucket_id >= BUCKET_SIZE {
                return Err(ExperimentError::InvalidParameter(
                    format!("Bucket {} exceeds max {}", bucket_id, BUCKET_SIZE)
                ));
            }
        }
        
        // Validate all bucket groups exist
        for group_id in self.buckets.values() {
            if !self.groups.contains_key(group_id) {
                return Err(ExperimentError::GroupNotFound(group_id.clone()));
            }
        }
        
        Ok(())
    }
    
    /// Get group for a bucket
    pub fn get_group(&self, bucket: u32) -> Result<&Group> {
        let group_id = self.buckets
            .get(&bucket)
            .ok_or_else(|| ExperimentError::BucketNotFound(bucket))?;
        
        self.groups
            .get(group_id)
            .ok_or_else(|| ExperimentError::GroupNotFound(group_id.clone()))
    }
}

/// Layer version tracking
#[derive(Debug, Clone)]
struct LayerVersion {
    layer: Arc<Layer>,
    file_path: PathBuf,
}

/// Layer Manager - manages all layers with hot reload support
pub struct LayerManager {
    pub(crate) layers_dir: PathBuf,
    
    /// layer_id -> LayerVersion
    layers: Arc<ArcSwap<HashMap<String, LayerVersion>>>,
    
    /// Rollback history: layer_id -> previous versions
    history: Arc<RwLock<HashMap<String, Vec<Arc<Layer>>>>>,
}

impl LayerManager {
    pub fn new(layers_dir: PathBuf) -> Self {
        Self {
            layers_dir,
            layers: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Load all layers from directory
    pub async fn load_all_layers(&self) -> Result<()> {
        let mut new_layers = HashMap::new();
        
        if !self.layers_dir.exists() {
            tracing::warn!("Layers directory does not exist: {:?}", self.layers_dir);
            return Ok(());
        }
        
        let entries = std::fs::read_dir(&self.layers_dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "json" || ext == "yaml" || ext == "yml" {
                        match Layer::from_file(&path) {
                            Ok(layer) => {
                                tracing::info!(
                                    "Loaded layer: {} (version: {}, priority: {})",
                                    layer.layer_id,
                                    layer.version,
                                    layer.priority
                                );
                                
                                new_layers.insert(
                                    layer.layer_id.clone(),
                                    LayerVersion {
                                        layer: Arc::new(layer),
                                        file_path: path.clone(),
                                    },
                                );
                            }
                            Err(e) => {
                                tracing::error!("Failed to load layer from {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
        }
        
        // Atomic swap
        self.layers.store(Arc::new(new_layers));
        
        Ok(())
    }
    
    /// Load or reload a single layer
    pub async fn load_layer(&self, layer_id: &str, file_path: &Path) -> Result<()> {
        let layer = Layer::from_file(file_path)?;
        
        // Verify layer_id matches
        if layer.layer_id != layer_id {
            return Err(ExperimentError::InvalidParameter(
                format!("Layer ID mismatch: expected {}, got {}", layer_id, layer.layer_id)
            ));
        }
        
        let current = self.layers.load();
        let mut new_layers = (**current).clone();
        
        // Save to history if updating
        if let Some(old_version) = new_layers.get(layer_id) {
            let mut history = self.history.write();
            history
                .entry(layer_id.to_string())
                .or_insert_with(Vec::new)
                .push(old_version.layer.clone());
            
            tracing::info!(
                "Updating layer {} from version {} to {}",
                layer_id,
                old_version.layer.version,
                layer.version
            );
        } else {
            tracing::info!("Adding new layer: {} (version: {})", layer_id, layer.version);
        }
        
        new_layers.insert(
            layer_id.to_string(),
            LayerVersion {
                layer: Arc::new(layer),
                file_path: file_path.to_path_buf(),
            },
        );
        
        // Atomic swap
        self.layers.store(Arc::new(new_layers));
        
        Ok(())
    }
    
    /// Remove a layer
    pub async fn remove_layer(&self, layer_id: &str) -> Result<()> {
        let current = self.layers.load();
        let mut new_layers = (**current).clone();
        
        if new_layers.remove(layer_id).is_some() {
            tracing::info!("Removed layer: {}", layer_id);
            self.layers.store(Arc::new(new_layers));
            Ok(())
        } else {
            Err(ExperimentError::LayerNotFound(layer_id.to_string()))
        }
    }
    
    /// Rollback layer to previous version
    pub async fn rollback_layer(&self, layer_id: &str) -> Result<()> {
        let mut history = self.history.write();
        
        if let Some(versions) = history.get_mut(layer_id) {
            if let Some(prev_layer) = versions.pop() {
                let current = self.layers.load();
                let mut new_layers = (**current).clone();
                
                if let Some(layer_version) = new_layers.get(layer_id) {
                    new_layers.insert(
                        layer_id.to_string(),
                        LayerVersion {
                            layer: prev_layer.clone(),
                            file_path: layer_version.file_path.clone(),
                        },
                    );
                    
                    self.layers.store(Arc::new(new_layers));
                    
                    tracing::info!("Rolled back layer {} to version {}", layer_id, prev_layer.version);
                    return Ok(());
                }
            }
        }
        
        Err(ExperimentError::InvalidVersion(format!(
            "No rollback version available for layer {}",
            layer_id
        )))
    }
    
    /// Get all layers sorted by priority (highest first)
    pub fn get_sorted_layers(&self) -> Vec<Arc<Layer>> {
        let layers = self.layers.load();
        let mut layer_list: Vec<_> = layers
            .values()
            .filter(|v| v.layer.enabled)
            .map(|v| v.layer.clone())
            .collect();
        
        // Sort by priority (descending) then by layer_id for determinism
        layer_list.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.layer_id.cmp(&b.layer_id))
        });
        
        layer_list
    }
    
    /// Get specific layer
    pub fn get_layer(&self, layer_id: &str) -> Option<Arc<Layer>> {
        self.layers.load().get(layer_id).map(|v| v.layer.clone())
    }
    
    /// Get all layer IDs
    pub fn get_layer_ids(&self) -> Vec<String> {
        self.layers.load().keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_layer_validation() {
        let layer = Layer {
            layer_id: "test".to_string(),
            version: "v1".to_string(),
            priority: 100,
            hash_key: "user_id".to_string(),
            salt: None,
            buckets: [(0, "group_a".to_string())].into_iter().collect(),
            groups: [(
                "group_a".to_string(),
                Group {
                    service: "test_svc".to_string(),
                    params: serde_json::json!({"key": "value"}),
                    rule: None,
                },
            )]
            .into_iter()
            .collect(),
            enabled: true,
        };
        
        assert!(layer.validate().is_ok());
    }
    
    #[tokio::test]
    async fn test_layer_manager_load() {
        let temp_dir = TempDir::new().unwrap();
        let layer_path = temp_dir.path().join("test_layer.json");
        
        let layer = Layer {
            layer_id: "test".to_string(),
            version: "v1".to_string(),
            priority: 100,
            hash_key: "user_id".to_string(),
            salt: None,
            buckets: [(0, "group_a".to_string())].into_iter().collect(),
            groups: [(
                "group_a".to_string(),
                Group {
                    service: "test_svc".to_string(),
                    params: serde_json::json!({"key": "value"}),
                    rule: None,
                },
            )]
            .into_iter()
            .collect(),
            enabled: true,
        };
        
        std::fs::write(&layer_path, serde_json::to_string_pretty(&layer).unwrap()).unwrap();
        
        let manager = LayerManager::new(temp_dir.path().to_path_buf());
        manager.load_all_layers().await.unwrap();
        
        let loaded = manager.get_layer("test").unwrap();
        assert_eq!(loaded.layer_id, "test");
        assert_eq!(loaded.version, "v1");
    }
}
