use crate::catalog::{ExperimentCatalog, VariantDef};
use crate::error::{ExperimentError, Result};
use arc_swap::ArcSwap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Bucket size (10000 slots = 0.01% granularity)
pub const BUCKET_SIZE: u32 = 10000;

/// Explicit bucket range mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BucketRange {
    pub start: u32,
    pub end: u32,
    pub vid: i64,
}

/// Layer definition (runtime)
#[derive(Debug, Clone, Serialize)]
pub struct Layer {
    pub layer_id: String,
    pub version: String,
    pub priority: i32,
    pub hash_key: String,

    /// Optional salt for hash calculation
    /// If not provided, defaults to "{layer_id}_{version}"
    #[serde(default)]
    pub salt: Option<String>,

    /// DEPRECATED: Services this layer may affect.
    /// Now inferred from catalog via ranges->vids during index build.
    /// Keep for backward compatibility but no longer used in new logic.
    #[serde(default)]
    pub services: Vec<String>,

    /// Slot ranges (half-open): start <= slot < end
    #[serde(default)]
    pub ranges: Vec<BucketRange>,

    #[serde(default)]
    pub enabled: bool,
}

/// Backward/forward compatible config schema.
///
/// - New format: `ranges: [{start,end,vid}, ...]` + `services: [...]`
/// - Backward compat: `buckets` + `groups` (boundary encoding) will be converted into `ranges`
#[derive(Debug, Clone, Deserialize)]
struct LayerConfig {
    pub layer_id: String,
    pub version: String,
    pub priority: i32,
    pub hash_key: String,

    #[serde(default)]
    pub salt: Option<String>,

    #[serde(default)]
    pub services: Vec<String>,

    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub ranges: Vec<BucketRangeConfig>,

    /// Deprecated: boundary buckets, converted into `ranges`
    #[serde(default)]
    pub buckets: HashMap<u32, String>,

    /// Deprecated: inline groups, used only to resolve legacy `buckets`/`ranges.group`
    #[serde(default)]
    pub groups: HashMap<String, VariantDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum BucketRangeConfig {
    Vid { start: u32, end: u32, vid: i64 },
    Group { start: u32, end: u32, group: String },
}

impl Layer {
    /// Get the salt for this layer.
    /// If salt is not configured, use "{layer_id}_{version}" as default.
    pub fn get_salt(&self) -> String {
        self.salt
            .clone()
            .unwrap_or_else(|| format!("{}_{}", self.layer_id, self.version))
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;

        // Try JSON first, then YAML
        let cfg: LayerConfig = serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content).map_err(ExperimentError::from))?;

        let layer = Self::try_from_config(cfg)?;

        Ok(layer)
    }

    fn try_from_config(mut cfg: LayerConfig) -> Result<Self> {
        // Normalize services (backward compat: keep if provided, but no longer required)
        cfg.services = normalize_services(cfg.services);
        // Note: services will be inferred from catalog during index build

        // Normalize ranges
        let mut ranges: Vec<BucketRange> = Vec::new();

        if !cfg.ranges.is_empty() {
            ranges = cfg
                .ranges
                .into_iter()
                .map(|r| resolve_range(r, &cfg.groups))
                .collect::<Result<Vec<_>>>()?;
        } else if !cfg.buckets.is_empty() {
            // Backward compat: treat buckets as boundary encoding
            ranges = convert_buckets_to_ranges(&cfg.buckets, &cfg.groups)?;
        }

        validate_and_sort_ranges(&mut ranges)?;

        Ok(Self {
            layer_id: cfg.layer_id,
            version: cfg.version,
            priority: cfg.priority,
            hash_key: cfg.hash_key,
            salt: cfg.salt,
            services: cfg.services,
            ranges,
            enabled: cfg.enabled,
        })
    }

    /// Get matched VID for a bucket/slot.
    ///
    /// Returns `None` when the slot is not covered by any range (hole/unoccupied).
    ///
    /// Uses binary search (O(log n)) since ranges are sorted by start.
    pub fn get_vid(&self, bucket: u32) -> Option<i64> {
        if bucket >= BUCKET_SIZE {
            return None;
        }

        // Binary search: find the first range where start > bucket
        let pos = self.ranges.partition_point(|r| r.start <= bucket);

        // Check if the previous range covers this bucket
        if pos > 0 {
            let candidate = &self.ranges[pos - 1];
            if bucket < candidate.end {
                return Some(candidate.vid);
            }
        }

        None
    }
}

fn normalize_services(services: Vec<String>) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    for s in services {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            continue;
        }
        set.insert(trimmed.to_string());
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

fn resolve_range(r: BucketRangeConfig, groups: &HashMap<String, VariantDef>) -> Result<BucketRange> {
    match r {
        BucketRangeConfig::Vid { start, end, vid } => Ok(BucketRange { start, end, vid }),
        BucketRangeConfig::Group { start, end, group } => {
            if let Ok(vid) = group.parse::<i64>() {
                return Ok(BucketRange { start, end, vid });
            }
            let def = groups
                .get(&group)
                .ok_or_else(|| ExperimentError::GroupNotFound(group.clone()))?;
            Ok(BucketRange {
                start,
                end,
                vid: def.vid,
            })
        }
    }
}

fn convert_buckets_to_ranges(
    buckets: &HashMap<u32, String>,
    groups: &HashMap<String, VariantDef>,
) -> Result<Vec<BucketRange>> {
    if buckets.is_empty() {
        return Ok(Vec::new());
    }

    let mut boundaries: Vec<(u32, String)> = buckets
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    boundaries.sort_by_key(|(k, _)| *k);

    let mut ranges = Vec::with_capacity(boundaries.len());

    for i in 0..boundaries.len() {
        let (start, group_id) = &boundaries[i];
        let end = if i + 1 < boundaries.len() {
            boundaries[i + 1].0
        } else {
            BUCKET_SIZE
        };

        let def = groups
            .get(group_id)
            .ok_or_else(|| ExperimentError::GroupNotFound(group_id.clone()))?;

        ranges.push(BucketRange {
            start: *start,
            end,
            vid: def.vid,
        });
    }

    Ok(ranges)
}

fn validate_and_sort_ranges(ranges: &mut Vec<BucketRange>) -> Result<()> {
    for r in ranges.iter() {
        if r.start >= r.end {
            return Err(ExperimentError::InvalidParameter(format!(
                "Invalid range: start {} must be < end {}",
                r.start, r.end
            )));
        }
        if r.end > BUCKET_SIZE {
            return Err(ExperimentError::InvalidParameter(format!(
                "Invalid range: end {} exceeds BUCKET_SIZE {}",
                r.end, BUCKET_SIZE
            )));
        }
    }

    // Sort for determinism and to enable overlap check
    ranges.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.end.cmp(&b.end)));

    // Check overlap
    for w in ranges.windows(2) {
        let prev = &w[0];
        let next = &w[1];
        if next.start < prev.end {
            return Err(ExperimentError::InvalidParameter(format!(
                "Overlapping ranges: [{}, {}) overlaps [{}, {})",
                prev.start, prev.end, next.start, next.end
            )));
        }
    }

    Ok(())
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

    /// Service → Layers inverted index for sparse matrix optimization
    /// service -> [layer_id] (sorted by priority)
    service_index: Arc<ArcSwap<HashMap<String, Vec<String>>>>,

    /// Rollback history: layer_id -> previous versions
    history: Arc<RwLock<HashMap<String, Vec<Arc<Layer>>>>>,
}

impl LayerManager {
    pub fn new(layers_dir: PathBuf) -> Self {
        Self {
            layers_dir,
            layers: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            service_index: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Rebuild service inverted index (inferred from catalog via ranges->vids)
    ///
    /// NEW LOGIC: For each layer, collect all vids from ranges, then reverse-query
    /// catalog (vid → eid → service) to determine which services this layer affects.
    fn rebuild_service_index(&self, layers_map: &HashMap<String, LayerVersion>, catalog: &ExperimentCatalog) {
        let mut service_to_layers: HashMap<String, Vec<(String, i32)>> = HashMap::new();

        for (layer_id, layer_ver) in layers_map {
            if !layer_ver.layer.enabled {
                continue;
            }

            // Collect all vids from ranges
            let vids: Vec<i64> = layer_ver.layer.ranges.iter().map(|r| r.vid).collect();

            // Reverse-query catalog to get services
            let mut services = std::collections::HashSet::new();
            for vid in vids {
                if let Some((_, service, _, _)) = catalog.get_variant(vid) {
                    services.insert(service.to_string());
                } else {
                    tracing::warn!(
                        "Layer {} references unknown vid {} (catalog may be incomplete)",
                        layer_id,
                        vid
                    );
                }
            }

            // Build inverted index
            for service in services {
                service_to_layers
                    .entry(service)
                    .or_insert_with(Vec::new)
                    .push((layer_id.clone(), layer_ver.layer.priority));
            }
        }

        // Sort by priority (descending) and layer_id (for determinism)
        let mut service_index: HashMap<String, Vec<String>> = HashMap::new();
        for (service, mut layer_list) in service_to_layers {
            layer_list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            service_index.insert(
                service,
                layer_list.into_iter().map(|(id, _)| id).collect(),
            );
        }

        self.service_index.store(Arc::new(service_index));
    }

    /// Load all layers from directory
    ///
    /// NOTE: This method now requires catalog to build service index.
    /// Caller must ensure catalog is loaded before calling this method.
    pub async fn load_all_layers(&self, catalog: &ExperimentCatalog) -> Result<()> {
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

        // Rebuild service index (now requires catalog)
        self.rebuild_service_index(&new_layers, catalog);

        // Atomic swap
        self.layers.store(Arc::new(new_layers));

        Ok(())
    }

    /// Load or reload a single layer
    pub async fn load_layer(&self, layer_id: &str, file_path: &Path, catalog: &ExperimentCatalog) -> Result<()> {
        let layer = Layer::from_file(file_path)?;

        // Verify layer_id matches
        if layer.layer_id != layer_id {
            return Err(ExperimentError::InvalidParameter(format!(
                "Layer ID mismatch: expected {}, got {}",
                layer_id, layer.layer_id
            )));
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

        // Rebuild service index (now requires catalog)
        self.rebuild_service_index(&new_layers, catalog);

        // Atomic swap
        self.layers.store(Arc::new(new_layers));

        Ok(())
    }

    /// Remove a layer
    pub async fn remove_layer(&self, layer_id: &str, catalog: &ExperimentCatalog) -> Result<()> {
        let current = self.layers.load();
        let mut new_layers = (**current).clone();

        if new_layers.remove(layer_id).is_some() {
            tracing::info!("Removed layer: {}", layer_id);

            // Rebuild service index (now requires catalog)
            self.rebuild_service_index(&new_layers, catalog);

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

                    tracing::info!(
                        "Rolled back layer {} to version {}",
                        layer_id,
                        prev_layer.version
                    );
                    return Ok(());
                }
            }
        }

        Err(ExperimentError::InvalidVersion(format!(
            "No rollback version available for layer {}",
            layer_id
        )))
    }

    /// Get specific layer
    pub fn get_layer(&self, layer_id: &str) -> Option<Arc<Layer>> {
        self.layers.load().get(layer_id).map(|v| v.layer.clone())
    }

    /// Get all layer IDs
    pub fn get_layer_ids(&self) -> Vec<String> {
        self.layers.load().keys().cloned().collect()
    }

    /// Get layers for a specific service (using inverted index)
    pub fn get_layers_for_service(&self, service: &str) -> Vec<Arc<Layer>> {
        let service_index = self.service_index.load();
        let layers = self.layers.load();

        if let Some(layer_ids) = service_index.get(service) {
            layer_ids
                .iter()
                .filter_map(|id| layers.get(id).map(|v| v.layer.clone()))
                .filter(|layer| layer.enabled)
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ranges_hit_and_hole() {
        let layer = Layer {
            layer_id: "test".to_string(),
            version: "v1".to_string(),
            priority: 100,
            hash_key: "user_id".to_string(),
            salt: None,
            services: vec!["svc".to_string()],
            ranges: vec![
                BucketRange {
                    start: 0,
                    end: 5000,
                    vid: 1,
                },
                BucketRange {
                    start: 7500,
                    end: 10000,
                    vid: 2,
                },
            ],
            enabled: true,
        };

        assert_eq!(layer.get_vid(0), Some(1));
        assert_eq!(layer.get_vid(4999), Some(1));
        assert_eq!(layer.get_vid(5000), None); // hole
        assert_eq!(layer.get_vid(7499), None); // hole
        assert_eq!(layer.get_vid(7500), Some(2));
        assert_eq!(layer.get_vid(9999), Some(2));
    }

    #[test]
    fn test_ranges_overlap_error() {
        let mut ranges = vec![
            BucketRange {
                start: 0,
                end: 10,
                vid: 1,
            },
            BucketRange {
                start: 5,
                end: 20,
                vid: 2,
            },
        ];

        let err = validate_and_sort_ranges(&mut ranges).unwrap_err();
        assert!(format!("{}", err).contains("Overlapping ranges"));
    }

    #[test]
    fn test_ranges_end_bound_error() {
        let mut ranges = vec![BucketRange {
            start: 0,
            end: BUCKET_SIZE + 1,
            vid: 1,
        }];

        let err = validate_and_sort_ranges(&mut ranges).unwrap_err();
        assert!(format!("{}", err).contains("exceeds BUCKET_SIZE"));
    }

    #[tokio::test]
    async fn test_layer_manager_load() {
        use crate::catalog::ExperimentDef;

        let temp_dir = TempDir::new().unwrap();
        let layer_path = temp_dir.path().join("test_layer.json");
        let groups_dir = temp_dir.path().join("groups");
        std::fs::create_dir_all(&groups_dir).unwrap();

        // Create dummy catalog
        let exp_def = ExperimentDef {
            eid: 100,
            service: "svc".to_string(),
            rule: None,
            variants: vec![VariantDef {
                vid: 1001,
                params: serde_json::json!({}),
            }],
        };
        std::fs::write(
            groups_dir.join("100.json"),
            serde_json::to_string_pretty(&exp_def).unwrap(),
        )
        .unwrap();
        let catalog = ExperimentCatalog::load_from_dir(groups_dir).unwrap();

        let layer = Layer {
            layer_id: "test".to_string(),
            version: "v1".to_string(),
            priority: 100,
            hash_key: "user_id".to_string(),
            salt: None,
            services: vec!["svc".to_string()],
            ranges: vec![BucketRange {
                start: 0,
                end: 1,
                vid: 1001,
            }],
            enabled: true,
        };

        std::fs::write(&layer_path, serde_json::to_string_pretty(&layer).unwrap()).unwrap();

        let manager = LayerManager::new(temp_dir.path().to_path_buf());
        manager.load_all_layers(&catalog).await.unwrap();

        let loaded = manager.get_layer("test").unwrap();
        assert_eq!(loaded.layer_id, "test");
        assert_eq!(loaded.version, "v1");
    }
}
