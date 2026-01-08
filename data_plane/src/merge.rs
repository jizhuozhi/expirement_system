use crate::catalog::ExperimentCatalog;
use crate::error::{ExperimentError, Result};
use crate::hash::hash_to_bucket;
use crate::layer::LayerManager;
use crate::rule::FieldType;
use serde_json::Value;
use std::collections::HashMap;

/// Experiment request
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExperimentRequest {
    pub services: Vec<String>,
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub layers: Vec<String>,
}

/// Per-service result
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceResult {
    pub parameters: Value,
    pub vids: Vec<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_layers: Vec<String>,
}

/// Experiment response
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExperimentResponse {
    pub results: HashMap<String, ServiceResult>,
}

/// Merge multiple layers for multiple services
pub fn merge_layers_batch(
    request: &ExperimentRequest,
    layer_manager: &LayerManager,
    catalog: &ExperimentCatalog,
    field_types: &HashMap<String, FieldType>,
) -> Result<ExperimentResponse> {
    let mut results = HashMap::new();

    for service in &request.services {
        let service_result =
            merge_layers_for_service(service, request, layer_manager, catalog, field_types)?;
        results.insert(service.clone(), service_result);
    }

    Ok(ExperimentResponse { results })
}

fn merge_layers_for_service(
    service: &str,
    request: &ExperimentRequest,
    layer_manager: &LayerManager,
    catalog: &ExperimentCatalog,
    field_types: &HashMap<String, FieldType>,
) -> Result<ServiceResult> {
    let mut final_params = serde_json::Map::new();
    let mut matched_vids = Vec::new();
    let mut matched_layers = Vec::new();

    let layers = if request.layers.is_empty() {
        layer_manager.get_layers_for_service(service)
    } else {
        request
            .layers
            .iter()
            .filter_map(|id| layer_manager.get_layer(id))
            .collect()
    };

    for layer in layers {
        let hash_key_value = match request.context.get(&layer.hash_key) {
            Some(Value::String(s)) => s.as_str(),
            Some(Value::Number(n)) => {
                tracing::warn!(
                    "Hash key '{}' is a number, converting to string for layer '{}'",
                    layer.hash_key,
                    layer.layer_id
                );
                &n.to_string()
            }
            Some(_) => {
                tracing::warn!(
                    "Hash key '{}' must be a string or number for layer '{}', skipping",
                    layer.hash_key,
                    layer.layer_id
                );
                continue;
            }
            None => {
                tracing::warn!(
                    "Hash key '{}' not found in context for layer '{}', skipping",
                    layer.hash_key,
                    layer.layer_id
                );
                continue;
            }
        };

        let salt = layer.get_salt();
        let bucket = hash_to_bucket(hash_key_value, &salt);

        let Some(vid) = layer.get_vid(bucket) else {
            continue;
        };

        let Some((eid, variant_service, rule_opt, params)) = catalog.get_variant(vid) else {
            tracing::warn!(
                "Missing vid {} in catalog (layer: {}, bucket: {}), skipping",
                vid,
                layer.layer_id,
                bucket
            );
            continue;
        };

        if variant_service != service {
            continue;
        }

        if let Some(rule) = rule_opt {
            let rule_passed = match rule.evaluate(&request.context, field_types) {
                Ok(passed) => passed,
                Err(e) => {
                    tracing::warn!(
                        "Rule evaluation failed for eid {} (layer {}, vid {}): {}",
                        eid,
                        layer.layer_id,
                        vid,
                        e
                    );
                    false
                }
            };

            if !rule_passed {
                continue;
            }
        }

        merge_params_prioritized(&mut final_params, params)?;
        matched_vids.push(vid);
        matched_layers.push(layer.layer_id.clone());
    }

    Ok(ServiceResult {
        parameters: Value::Object(final_params),
        vids: matched_vids,
        matched_layers,
    })
}

/// Merge parameters with priority (higher priority layer wins for same keys)
fn merge_params_prioritized(target: &mut serde_json::Map<String, Value>, source: &Value) -> Result<()> {
    match source {
        Value::Object(source_map) => {
            for (key, value) in source_map {
                match (target.get_mut(key), value) {
                    (Some(Value::Object(target_obj)), Value::Object(source_obj)) => {
                        let mut target_map = target_obj.clone();
                        merge_params_prioritized(&mut target_map, &Value::Object(source_obj.clone()))?;
                        target.insert(key.clone(), Value::Object(target_map));
                    }
                    (Some(_), _) => {}
                    (None, _) => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        _ => {
            return Err(ExperimentError::InvalidParameter(
                "Source must be an object".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{ExperimentCatalog, ExperimentDef, VariantDef};
    use crate::layer::{BucketRange, Layer, LayerManager};
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_merge_params_nested() {
        let mut target = serde_json::Map::new();
        target.insert("a".to_string(), json!({"x": 1}));
        target.insert("b".to_string(), json!(2));

        let source = json!({
            "a": {"y": 2},
            "c": 3
        });

        merge_params_prioritized(&mut target, &source).unwrap();

        assert_eq!(target.get("a"), Some(&json!({"x": 1, "y": 2})));
        assert_eq!(target.get("b"), Some(&json!(2)));
        assert_eq!(target.get("c"), Some(&json!(3)));
    }

    #[test]
    fn test_merge_params_override() {
        let mut target = serde_json::Map::new();
        target.insert("key".to_string(), json!("high_priority"));

        let source = json!({"key": "low_priority"});

        merge_params_prioritized(&mut target, &source).unwrap();

        assert_eq!(target.get("key"), Some(&json!("high_priority")));
    }

    #[tokio::test]
    async fn test_merge_layers_batch() {
        let temp_dir = TempDir::new().unwrap();
        let layers_dir = temp_dir.path().join("layers");
        let experiments_dir = temp_dir.path().join("experiments");
        std::fs::create_dir_all(&layers_dir).unwrap();
        std::fs::create_dir_all(&experiments_dir).unwrap();

        // Create test experiment with variants
        let exp1 = ExperimentDef {
            eid: 100,
            service: "test_svc".to_string(),
            rule: None,
            variants: vec![
                VariantDef {
                    vid: 1001,
                    params: json!({"feature_a": true, "timeout": 100}),
                },
                VariantDef {
                    vid: 1002,
                    params: json!({"feature_b": true, "timeout": 200}),
                },
            ],
        };
        std::fs::write(
            experiments_dir.join("100.json"),
            serde_json::to_string_pretty(&exp1).unwrap(),
        )
        .unwrap();

        let catalog = ExperimentCatalog::load_from_dir(experiments_dir).unwrap();

        // Create test layers
        let test_user = "user_test_123";
        let layer1_salt = "layer1_v1";
        let layer2_salt = "layer2_v1";
        let bucket1 = crate::hash::hash_to_bucket(test_user, layer1_salt);
        let bucket2 = crate::hash::hash_to_bucket(test_user, layer2_salt);

        let layer1 = Layer {
            layer_id: "layer1".to_string(),
            version: "v1".to_string(),
            priority: 200,
            hash_key: "user_id".to_string(),
            salt: Some(layer1_salt.to_string()),
            services: vec![],
            ranges: vec![BucketRange {
                start: bucket1,
                end: bucket1.saturating_add(1).min(crate::layer::BUCKET_SIZE),
                vid: 1001,
            }],
            enabled: true,
        };

        let layer2 = Layer {
            layer_id: "layer2".to_string(),
            version: "v1".to_string(),
            priority: 100,
            hash_key: "user_id".to_string(),
            salt: Some(layer2_salt.to_string()),
            services: vec![],
            ranges: vec![BucketRange {
                start: bucket2,
                end: bucket2.saturating_add(1).min(crate::layer::BUCKET_SIZE),
                vid: 1002,
            }],
            enabled: true,
        };

        std::fs::write(
            layers_dir.join("layer1.json"),
            serde_json::to_string_pretty(&layer1).unwrap(),
        )
        .unwrap();
        std::fs::write(
            layers_dir.join("layer2.json"),
            serde_json::to_string_pretty(&layer2).unwrap(),
        )
        .unwrap();

        // Create layer manager
        let manager = LayerManager::new(layers_dir);
        manager.load_all_layers(&catalog).await.unwrap();

        let request = ExperimentRequest {
            services: vec!["test_svc".to_string()],
            context: [
                ("user_id".to_string(), json!(test_user)),
            ]
            .into_iter()
            .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();
        let response = merge_layers_batch(&request, &manager, &catalog, &field_types).unwrap();

        let result = response.results.get("test_svc").unwrap();

        // layer1 has higher priority, so its timeout should win
        assert_eq!(result.parameters["timeout"], json!(100));
        assert_eq!(result.parameters["feature_a"], json!(true));
        assert_eq!(result.parameters["feature_b"], json!(true));
        assert_eq!(result.vids, vec![1001, 1002]);
        assert_eq!(result.matched_layers.len(), 2);
    }
}
