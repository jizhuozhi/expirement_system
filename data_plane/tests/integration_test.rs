use experiment_data_plane::catalog::{ExperimentCatalog, ExperimentDef, VariantDef};
use experiment_data_plane::hash::hash_to_bucket;
use experiment_data_plane::layer::{BucketRange, Layer, LayerManager, BUCKET_SIZE};
use experiment_data_plane::merge::{merge_layers_batch, ExperimentRequest};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_layer_loading_and_service_index() {
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&layers_dir).unwrap();
    std::fs::create_dir_all(&experiments_dir).unwrap();

    // Test implementation
    let exp = ExperimentDef {
        eid: 100,
        service: "test_service".to_string(),
        rule: None,
        variants: vec![
            VariantDef {
                vid: 1001,
                params: json!({"feature": "a"}),
            },
            VariantDef {
                vid: 1002,
                params: json!({"feature": "b"}),
            },
        ],
    };
    std::fs::write(
        experiments_dir.join("100.json"),
        serde_json::to_string_pretty(&exp).unwrap(),
    )
    .unwrap();

    let catalog = ExperimentCatalog::load_from_dir(experiments_dir).unwrap();
    assert_eq!(catalog.len(), 1);

    // Test implementation
    let layer = Layer {
        layer_id: "test_layer".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: None,
        services: vec![],
        ranges: vec![
            BucketRange {
                start: 0,
                end: 5000,
                vid: 1001,
            },
            BucketRange {
                start: 5000,
                end: 10000,
                vid: 1002,
            },
        ],
        enabled: true,
    };

    std::fs::write(
        layers_dir.join("test_layer.json"),
        serde_json::to_string_pretty(&layer).unwrap(),
    )
    .unwrap();

    let manager = LayerManager::new(layers_dir);
    manager.load_all_layers(&catalog).await.unwrap();

    // Test implementation
    let layers = manager.get_layers_for_service("test_service");
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].layer_id, "test_layer");
}

#[tokio::test]
async fn test_merge_with_ranges() {
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&layers_dir).unwrap();
    std::fs::create_dir_all(&experiments_dir).unwrap();

    // Test implementation
    let exp = ExperimentDef {
        eid: 200,
        service: "api".to_string(),
        rule: None,
        variants: vec![
            VariantDef {
                vid: 2001,
                params: json!({"timeout": 100, "retries": 3}),
            },
            VariantDef {
                vid: 2002,
                params: json!({"timeout": 200, "cache": true}),
            },
        ],
    };
    std::fs::write(
        experiments_dir.join("200.json"),
        serde_json::to_string_pretty(&exp).unwrap(),
    )
    .unwrap();

    let catalog = Arc::new(ExperimentCatalog::load_from_dir(experiments_dir).unwrap());

    // Test implementation
    let test_user = "user_123";
    let salt = "test_salt";
    let bucket = hash_to_bucket(test_user, salt);

    let layer = Layer {
        layer_id: "api_layer".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt.to_string()),
        services: vec![],
        ranges: vec![BucketRange {
            start: bucket,
            end: bucket.saturating_add(1).min(BUCKET_SIZE),
            vid: 2001,
        }],
        enabled: true,
    };

    std::fs::write(
        layers_dir.join("api_layer.json"),
        serde_json::to_string_pretty(&layer).unwrap(),
    )
    .unwrap();

    let manager = LayerManager::new(layers_dir);
    manager.load_all_layers(&catalog).await.unwrap();

    let request = ExperimentRequest {
        services: vec!["api".to_string()],
        context: [("user_id".to_string(), json!(test_user))]
            .into_iter()
            .collect(),
        layers: vec![],
    };

    let field_types = HashMap::new();
    let response = merge_layers_batch(&request, &manager, &catalog, &field_types).unwrap();

    let result = response.results.get("api").unwrap();
    assert_eq!(result.vids, vec![2001]);
    assert_eq!(result.parameters["timeout"], json!(100));
    assert_eq!(result.parameters["retries"], json!(3));
}

#[tokio::test]
async fn test_eid_rule_evaluation_memo() {
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&layers_dir).unwrap();
    std::fs::create_dir_all(&experiments_dir).unwrap();

    // Test implementation
    let exp = ExperimentDef {
        eid: 300,
        service: "api".to_string(),
        rule: Some(experiment_data_plane::rule::Node::Field {
            field: "region".to_string(),
            op: experiment_data_plane::rule::Op::Eq,
            values: vec![json!("US")],
        }),
        variants: vec![
            VariantDef {
                vid: 3001,
                params: json!({"feature": "a"}),
            },
            VariantDef {
                vid: 3002,
                params: json!({"feature": "b"}),
            },
        ],
    };
    std::fs::write(
        experiments_dir.join("300.json"),
        serde_json::to_string_pretty(&exp).unwrap(),
    )
    .unwrap();

    let catalog = Arc::new(ExperimentCatalog::load_from_dir(experiments_dir).unwrap());

    let test_user = "user_456";
    let salt1 = "layer1_salt";
    let salt2 = "layer2_salt";
    let bucket1 = hash_to_bucket(test_user, salt1);
    let bucket2 = hash_to_bucket(test_user, salt2);

    // Test implementation
    let layer1 = Layer {
        layer_id: "layer1".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: Some(salt1.to_string()),
        services: vec![],
        ranges: vec![BucketRange {
            start: bucket1,
            end: bucket1.saturating_add(1).min(BUCKET_SIZE),
            vid: 3001,
        }],
        enabled: true,
    };

    let layer2 = Layer {
        layer_id: "layer2".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt2.to_string()),
        services: vec![],
        ranges: vec![BucketRange {
            start: bucket2,
            end: bucket2.saturating_add(1).min(BUCKET_SIZE),
            vid: 3002,
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

    let manager = LayerManager::new(layers_dir);
    manager.load_all_layers(&catalog).await.unwrap();

    // Test implementation
    let mut context = HashMap::new();
    context.insert("user_id".to_string(), json!(test_user));
    context.insert("region".to_string(), json!("US"));

    let request = ExperimentRequest {
        services: vec!["api".to_string()],
        context,
        layers: vec![],
    };

    let mut field_types = HashMap::new();
    field_types.insert(
        "region".to_string(),
        experiment_data_plane::rule::FieldType::String,
    );

    let response = merge_layers_batch(&request, &manager, &catalog, &field_types).unwrap();

    let result = response.results.get("api").unwrap();
    // Test implementation
    assert_eq!(result.vids.len(), 2);
    assert!(result.vids.contains(&3001));
    assert!(result.vids.contains(&3002));
}
