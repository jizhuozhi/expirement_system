use experiment_data_plane::catalog::{ExperimentCatalog, ExperimentDef, VariantDef};
use experiment_data_plane::hash::hash_to_bucket;
use experiment_data_plane::layer::{BucketRange, Layer, LayerManager, BUCKET_SIZE};
use experiment_data_plane::merge::{merge_layers_batch, ExperimentRequest};
use experiment_data_plane::rule::{FieldType, Node, Op};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_rule_based_experiment() {
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&layers_dir).unwrap();
    std::fs::create_dir_all(&experiments_dir).unwrap();

    let exp = ExperimentDef {
        eid: 400,
        service: "api".to_string(),
        rule: Some(Node::Field {
            field: "country".to_string(),
            op: Op::Eq,
            values: vec![json!("CN")],
        }),
        variants: vec![VariantDef {
            vid: 4001,
            params: json!({"feature": "china_special"}),
        }],
    };

    std::fs::write(
        experiments_dir.join("400.json"),
        serde_json::to_string_pretty(&exp).unwrap(),
    )
    .unwrap();

    let catalog = Arc::new(ExperimentCatalog::load_from_dir(experiments_dir).unwrap());

    let test_user = "user_cn";
    let salt = "test_salt";
    let bucket = hash_to_bucket(test_user, salt);

    let layer = Layer {
        layer_id: "geo_layer".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt.to_string()),
        services: vec![],
        ranges: vec![BucketRange {
            start: bucket,
            end: bucket.saturating_add(1).min(BUCKET_SIZE),
            vid: 4001,
        }],
        enabled: true,
    };

    std::fs::write(
        layers_dir.join("geo_layer.json"),
        serde_json::to_string_pretty(&layer).unwrap(),
    )
    .unwrap();

    let manager = LayerManager::new(layers_dir);
    manager.load_all_layers(&catalog).await.unwrap();

    // Test implementation
    {
        let mut context = HashMap::new();
        context.insert("user_id".to_string(), json!(test_user));
        context.insert("country".to_string(), json!("CN"));

        let request = ExperimentRequest {
            services: vec!["api".to_string()],
            context,
            layers: vec![],
        };

        let mut field_types = HashMap::new();
        field_types.insert("country".to_string(), FieldType::String);

        let response = merge_layers_batch(&request, &manager, &catalog, &field_types).unwrap();
        let result = response.results.get("api").unwrap();

        assert_eq!(result.vids, vec![4001]);
        assert_eq!(result.parameters["feature"], json!("china_special"));
    }

    // Test implementation
    {
        let mut context = HashMap::new();
        context.insert("user_id".to_string(), json!(test_user));
        context.insert("country".to_string(), json!("US"));

        let request = ExperimentRequest {
            services: vec!["api".to_string()],
            context,
            layers: vec![],
        };

        let mut field_types = HashMap::new();
        field_types.insert("country".to_string(), FieldType::String);

        let response = merge_layers_batch(&request, &manager, &catalog, &field_types).unwrap();
        let result = response.results.get("api").unwrap();

        // Test implementation
        assert_eq!(result.vids.len(), 0);
    }
}
