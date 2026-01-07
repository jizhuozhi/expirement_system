use crate::error::{ExperimentError, Result};
use crate::hash::hash_to_bucket_with_salt;
use crate::layer::Layer;
use crate::rule::FieldType;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Experiment request
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExperimentRequest {
    pub service: String,
    pub hash_keys: HashMap<String, String>,
    
    #[serde(default)]
    pub layers: Vec<String>, // Optional: specific layers to use
    
    /// Context for rule evaluation
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

/// Experiment response
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExperimentResponse {
    pub service: String,
    pub parameters: Value,
    pub matched_layers: Vec<String>,
}

/// Merge multiple layers to get final parameters
pub fn merge_layers(
    request: &ExperimentRequest,
    layers: &[Arc<Layer>],
    field_types: &HashMap<String, FieldType>,
) -> Result<ExperimentResponse> {
    let mut final_params = serde_json::Map::new();
    let mut matched_layers = Vec::new();
    
    // Filter layers by service and optional layer list
    let applicable_layers: Vec<_> = layers
        .iter()
        .filter(|layer| {
            // If specific layers requested, only use those
            if !request.layers.is_empty() {
                return request.layers.contains(&layer.layer_id);
            }
            true
        })
        .collect();
    
    // Process layers in priority order (already sorted by LayerManager)
    for layer in applicable_layers {
        // Get hash key value
        let hash_key_value = request
            .hash_keys
            .get(&layer.hash_key)
            .ok_or_else(|| ExperimentError::HashKeyNotFound(layer.hash_key.clone()))?;
        
        // Calculate bucket with layer-specific salt
        let salt = layer.get_salt();
        let bucket = hash_to_bucket_with_salt(hash_key_value, &salt);
        
        // Get group for this bucket
        let group = match layer.get_group(bucket) {
            Ok(g) => g,
            Err(_) => continue, // Skip if no group assigned to this bucket
        };
        
        // Check service constraint
        if group.service != request.service {
            continue; // Skip this layer if service doesn't match
        }
        
        // Evaluate rule if present
        if let Some(rule) = &group.rule {
            match rule.evaluate(&request.context, field_types) {
                Ok(true) => {
                    // Rule passed, continue
                }
                Ok(false) => {
                    // Rule failed, skip this group
                    continue;
                }
                Err(e) => {
                    // Rule evaluation error
                    tracing::warn!(
                        "Rule evaluation failed for layer {} group: {}",
                        layer.layer_id,
                        e
                    );
                    continue;
                }
            }
        }
        
        // Merge parameters
        merge_params(&mut final_params, &group.params)?;
        matched_layers.push(layer.layer_id.clone());
    }
    
    Ok(ExperimentResponse {
        service: request.service.clone(),
        parameters: Value::Object(final_params),
        matched_layers,
    })
}

/// Merge parameters with deterministic rules
fn merge_params(target: &mut serde_json::Map<String, Value>, source: &Value) -> Result<()> {
    match source {
        Value::Object(source_map) => {
            for (key, value) in source_map {
                match (target.get_mut(key), value) {
                    // Both are objects - recursively merge
                    (Some(Value::Object(target_obj)), Value::Object(source_obj)) => {
                        let mut target_map = target_obj.clone();
                        merge_params(&mut target_map, &Value::Object(source_obj.clone()))?;
                        target.insert(key.clone(), Value::Object(target_map));
                    }
                    // Source wins for scalars and arrays (higher priority layer)
                    (Some(_), _) => {
                        // Higher priority layer already set this, skip
                    }
                    // New key - insert
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
    use crate::layer::Group;
    use serde_json::json;
    
    #[test]
    fn test_merge_params_nested() {
        let mut target = serde_json::Map::new();
        target.insert("a".to_string(), json!({"x": 1}));
        target.insert("b".to_string(), json!(2));
        
        let source = json!({
            "a": {"y": 2},
            "c": 3
        });
        
        merge_params(&mut target, &source).unwrap();
        
        assert_eq!(target.get("a"), Some(&json!({"x": 1, "y": 2})));
        assert_eq!(target.get("b"), Some(&json!(2)));
        assert_eq!(target.get("c"), Some(&json!(3)));
    }
    
    #[test]
    fn test_merge_params_override() {
        let mut target = serde_json::Map::new();
        target.insert("key".to_string(), json!("high_priority"));
        
        let source = json!({"key": "low_priority"});
        
        merge_params(&mut target, &source).unwrap();
        
        // Higher priority value should remain
        assert_eq!(target.get("key"), Some(&json!("high_priority")));
    }
    
    #[test]
    fn test_merge_layers_integration() {
        use crate::hash::hash_to_bucket_with_salt;
        
        // We need to find a user_id that maps to bucket 0 for both layers
        // Or use specific buckets that the user maps to
        let test_user = "user_test_123";
        
        // Calculate which buckets this user will be in for each layer
        let layer1_salt = "layer1_v1";
        let layer2_salt = "layer2_v1";
        let bucket1 = hash_to_bucket_with_salt(test_user, layer1_salt);
        let bucket2 = hash_to_bucket_with_salt(test_user, layer2_salt);
        
        let layer1 = Arc::new(Layer {
            layer_id: "layer1".to_string(),
            version: "v1".to_string(),
            priority: 200, // Higher priority
            hash_key: "user_id".to_string(),
            salt: Some(layer1_salt.to_string()),
            buckets: [(bucket1, "group_a".to_string())].into_iter().collect(),
            groups: [(
                "group_a".to_string(),
                Group {
                    service: "test_svc".to_string(),
                    params: json!({"feature_a": true, "timeout": 100}),
                    rule: None,
                },
            )]
            .into_iter()
            .collect(),
            enabled: true,
        });
        
        let layer2 = Arc::new(Layer {
            layer_id: "layer2".to_string(),
            version: "v1".to_string(),
            priority: 100, // Lower priority
            hash_key: "user_id".to_string(),
            salt: Some(layer2_salt.to_string()),
            buckets: [(bucket2, "group_b".to_string())].into_iter().collect(),
            groups: [(
                "group_b".to_string(),
                Group {
                    service: "test_svc".to_string(),
                    params: json!({"feature_b": true, "timeout": 200}),
                    rule: None,
                },
            )]
            .into_iter()
            .collect(),
            enabled: true,
        });
        
        let request = ExperimentRequest {
            service: "test_svc".to_string(),
            hash_keys: [("user_id".to_string(), test_user.to_string())]
                .into_iter()
                .collect(),
            layers: vec![],
            context: HashMap::new(),
        };
        
        let field_types = HashMap::new();
        let response = merge_layers(&request, &[layer1, layer2], &field_types).unwrap();
        
        // layer1 has higher priority, so its timeout should win
        assert_eq!(response.parameters["timeout"], json!(100));
        assert_eq!(response.parameters["feature_a"], json!(true));
        assert_eq!(response.parameters["feature_b"], json!(true));
        assert_eq!(response.matched_layers.len(), 2);
    }
}
