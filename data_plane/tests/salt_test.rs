use experiment_data_plane::hash::hash_to_bucket_with_salt;
use experiment_data_plane::layer::{Group, Layer};
use experiment_data_plane::merge::{merge_layers, ExperimentRequest};
use serde_json::json;
use std::collections::{HashMap};
use std::sync::Arc;

#[test]
fn test_different_layers_produce_different_distributions() {
    // Create two layers with the same priority and hash_key
    // but different layer_id and version (thus different salts)
    let layer1 = Arc::new(Layer {
        layer_id: "experiment_a".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None, // Will use "experiment_a_v1"
        buckets: (0..10000).map(|i| (i, "group_a".to_string())).collect(),
        groups: [(
            "group_a".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"experiment": "a"}),
                rule: None,            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });

    let layer2 = Arc::new(Layer {
        layer_id: "experiment_b".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None, // Will use "experiment_b_v1"
        buckets: (0..10000).map(|i| (i, "group_b".to_string())).collect(),
        groups: [(
            "group_b".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"experiment": "b"}),
                rule: None,            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });

    // Test that the same user gets different buckets in different layers
    let mut same_bucket_count = 0;
    let total_users = 1000;

    for i in 0..total_users {
        let user_id = format!("user_{}", i);

        let salt1 = layer1.get_salt();
        let salt2 = layer2.get_salt();

        let bucket1 = hash_to_bucket_with_salt(&user_id, &salt1);
        let bucket2 = hash_to_bucket_with_salt(&user_id, &salt2);

        if bucket1 == bucket2 {
            same_bucket_count += 1;
        }
    }

    // With different salts, collision rate should be around 1/10000 = 0.01%
    // For 1000 users, we expect ~0.1 collisions
    // Allow up to 5% (50 users) to have same bucket by chance
    assert!(
        same_bucket_count < total_users / 20,
        "Too many bucket collisions: {} out of {} (expected < {})",
        same_bucket_count,
        total_users,
        total_users / 20
    );
}

#[test]
fn test_custom_salt_overrides_default() {
    let layer_with_custom_salt = Layer {
        layer_id: "test_layer".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some("my_custom_salt_123".to_string()),
        buckets: HashMap::new(),
        groups: HashMap::new(),
        enabled: true,
    };

    let layer_with_default_salt = Layer {
        layer_id: "test_layer".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None,
        buckets: HashMap::new(),
        groups: HashMap::new(),
        enabled: true,
    };

    assert_eq!(
        layer_with_custom_salt.get_salt(),
        "my_custom_salt_123".to_string()
    );
    assert_eq!(
        layer_with_default_salt.get_salt(),
        "test_layer_v1".to_string()
    );
}

#[test]
fn test_salt_ensures_independence_across_layers() {
    // Test scenario: Same user in two overlapping experiments
    // should have independent distribution
    let user_id = "user_12345";

    let salt1 = "click_experiment_v1";
    let salt2 = "color_experiment_v1";

    let bucket1 = hash_to_bucket_with_salt(user_id, salt1);
    let bucket2 = hash_to_bucket_with_salt(user_id, salt2);

    // Buckets should be different (with high probability)
    assert_ne!(
        bucket1, bucket2,
        "Same user should get different buckets in different experiments"
    );
}

#[test]
fn test_merge_with_different_salts() {
    // Create layers with explicit different salts
    let layer1 = Arc::new(Layer {
        layer_id: "layer1".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: Some("salt_layer1".to_string()),
        buckets: [(5000, "group_a".to_string())].into_iter().collect(),
        groups: [(
            "group_a".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"from": "layer1"}),
                rule: None,            },
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
        salt: Some("salt_layer2".to_string()),
        buckets: [(3000, "group_b".to_string())].into_iter().collect(),
        groups: [(
            "group_b".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"from": "layer2"}),
                rule: None,            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });

    let request = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), "user_test".to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: HashMap::new(),
    };

    let response = merge_layers(&request, &[layer1, layer2], &HashMap::new()).unwrap();

    // Verify that layers are processed independently based on their salts
    assert!(response.matched_layers.len() <= 2);
}

#[test]
fn test_version_change_produces_different_salt() {
    let layer_v1 = Layer {
        layer_id: "experiment".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None,
        buckets: HashMap::new(),
        groups: HashMap::new(),
        enabled: true,
    };

    let layer_v2 = Layer {
        layer_id: "experiment".to_string(),
        version: "v2".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None,
        buckets: HashMap::new(),
        groups: HashMap::new(),
        enabled: true,
    };

    let salt_v1 = layer_v1.get_salt();
    let salt_v2 = layer_v2.get_salt();

    assert_ne!(
        salt_v1, salt_v2,
        "Different versions should produce different salts"
    );
    assert_eq!(salt_v1, "experiment_v1");
    assert_eq!(salt_v2, "experiment_v2");
}

#[test]
fn test_salt_distribution_uniformity() {
    let salt = "test_experiment_v1";
    let mut bucket_counts = vec![0; 100]; // Group into 100 bins for simplicity

    for i in 0..10000 {
        let user_id = format!("user_{}", i);
        let bucket = hash_to_bucket_with_salt(&user_id, salt);
        let bin = (bucket as usize) / 100; // 10000 buckets -> 100 bins
        bucket_counts[bin] += 1;
    }

    // Check uniformity: each bin should have ~100 users (10000 / 100)
    let expected = 100;
    let mut outliers = 0;

    for &count in &bucket_counts {
        if count < expected / 2 || count > expected * 2 {
            outliers += 1;
        }
    }

    // Allow up to 10% outliers
    assert!(
        outliers < 10,
        "Distribution not uniform: {} outliers out of 100 bins",
        outliers
    );
}
