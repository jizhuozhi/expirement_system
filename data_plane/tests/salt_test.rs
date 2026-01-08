use experiment_data_plane::hash::hash_to_bucket;
use experiment_data_plane::layer::{BucketRange, Layer, BUCKET_SIZE};

#[test]
fn test_salt_isolation() {
    let key = "user_123";

    let salt1 = "experiment_a";
    let salt2 = "experiment_b";

    let bucket1 = hash_to_bucket(key, salt1);
    let bucket2 = hash_to_bucket(key, salt2);

    // Different salts should produce different buckets (most of the time)
    assert_ne!(bucket1, bucket2);
    assert!(bucket1 < BUCKET_SIZE);
    assert!(bucket2 < BUCKET_SIZE);
}

#[test]
fn test_layer_get_salt() {
    // Test explicit salt
    let layer1 = Layer {
        layer_id: "test".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some("custom_salt".to_string()),
        services: vec![],
        ranges: vec![],
        enabled: true,
    };
    assert_eq!(layer1.get_salt(), "custom_salt");

    // Test default salt (layer_id_version)
    let layer2 = Layer {
        layer_id: "test2".to_string(),
        version: "v2".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: None,
        services: vec![],
        ranges: vec![],
        enabled: true,
    };
    assert_eq!(layer2.get_salt(), "test2_v2");
}

#[test]
fn test_ranges_deterministic_hit() {
    let layer = Layer {
        layer_id: "deterministic".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some("fixed_salt".to_string()),
        services: vec![],
        ranges: vec![
            BucketRange {
                start: 0,
                end: 5000,
                vid: 1,
            },
            BucketRange {
                start: 5000,
                end: 10000,
                vid: 2,
            },
        ],
        enabled: true,
    };

    let key = "consistent_user";
    let bucket = hash_to_bucket(key, &layer.get_salt());

    // Multiple calls should return same vid
    let vid1 = layer.get_vid(bucket);
    let vid2 = layer.get_vid(bucket);
    assert_eq!(vid1, vid2);
    assert!(vid1.is_some());
}
