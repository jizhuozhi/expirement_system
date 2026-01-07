use experiment_data_plane::hash::hash_to_bucket_with_salt;
use experiment_data_plane::layer::{Group, Layer};
use experiment_data_plane::merge::{merge_layers, ExperimentRequest};
use experiment_data_plane::rule::FieldType;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_merge_priority() {
    let test_user = "user_priority_test";
    
    let salt_high = "high_v1";
    let salt_low = "low_v1";
    let bucket_high = hash_to_bucket_with_salt(test_user, salt_high);
    let bucket_low = hash_to_bucket_with_salt(test_user, salt_low);
    
    let layer_high = Arc::new(Layer {
        layer_id: "high".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: Some(salt_high.to_string()),
        buckets: [(bucket_high, "group_high".to_string())].into_iter().collect(),
        groups: [(
            "group_high".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({
                    "feature_a": "high_priority_value",
                    "feature_high_only": "high_value"
                }),
                rule: None,
            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });
    
    let layer_low = Arc::new(Layer {
        layer_id: "low".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt_low.to_string()),
        buckets: [(bucket_low, "group_low".to_string())].into_iter().collect(),
        groups: [(
            "group_low".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({
                    "feature_a": "low_priority_value",
                    "feature_low_only": "low_value"
                }),
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
    
    let response = merge_layers(&request, &[layer_high, layer_low], &HashMap::new()).unwrap();
    
    assert_eq!(response.parameters["feature_a"], json!("high_priority_value"));
    assert_eq!(response.parameters["feature_high_only"], json!("high_value"));
    assert_eq!(response.parameters["feature_low_only"], json!("low_value"));
}

#[test]
fn test_service_constraint() {
    let layer = Arc::new(Layer {
        layer_id: "test".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None,
        buckets: [(0, "group_a".to_string())].into_iter().collect(),
        groups: [(
            "group_a".to_string(),
            Group {
                service: "recommendation_svc".to_string(),
                params: json!({"feature": true}),
                rule: None,
            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });
    
    let request = ExperimentRequest {
        service: "search_svc".to_string(),
        hash_keys: [("user_id".to_string(), "user_0".to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: HashMap::new(),
    };
    
    let response = merge_layers(&request, &[layer], &HashMap::new()).unwrap();
    
    assert!(response.parameters.as_object().unwrap().is_empty());
    assert_eq!(response.matched_layers.len(), 0);
}

#[test]
fn test_nested_params_merge() {
    let test_user = "user_nested_test";
    
    let salt1 = "layer1_v1";
    let salt2 = "layer2_v1";
    let bucket1 = hash_to_bucket_with_salt(test_user, salt1);
    let bucket2 = hash_to_bucket_with_salt(test_user, salt2);
    
    let layer1 = Arc::new(Layer {
        layer_id: "layer1".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: Some(salt1.to_string()),
        buckets: [(bucket1, "group1".to_string())].into_iter().collect(),
        groups: [(
            "group1".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({
                    "config": {
                        "timeout": 100,
                        "high_priority_setting": "value1"
                    }
                }),
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
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt2.to_string()),
        buckets: [(bucket2, "group2".to_string())].into_iter().collect(),
        groups: [(
            "group2".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({
                    "config": {
                        "timeout": 200,
                        "low_priority_setting": "value2"
                    }
                }),
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
    
    let response = merge_layers(&request, &[layer1, layer2], &HashMap::new()).unwrap();
    
    let config = response.parameters["config"].as_object().unwrap();
    assert_eq!(config["timeout"], json!(100));
    assert_eq!(config["high_priority_setting"], json!("value1"));
    assert_eq!(config["low_priority_setting"], json!("value2"));
}
