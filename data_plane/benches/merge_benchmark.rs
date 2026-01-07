use criterion::{black_box, criterion_group, criterion_main, Criterion};
use experiment_data_plane::layer::{Group, Layer};
use experiment_data_plane::merge::{merge_layers, ExperimentRequest};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn create_test_layer(layer_id: &str, priority: i32, service: &str) -> Arc<Layer> {
    let mut buckets = HashMap::new();
    for i in 0..10 {
        buckets.insert(i, format!("group_{}", i % 3));
    }
    
    let mut groups = HashMap::new();
    for i in 0..3 {
        groups.insert(
            format!("group_{}", i),
            Group {
                service: service.to_string(),
                params: json!({
                    "feature_a": i,
                    "feature_b": format!("value_{}", i),
                    "nested": {
                        "x": i * 10,
                        "y": i * 20
                    }
                }),
            },
        );
    }
    
    Arc::new(Layer {
        layer_id: layer_id.to_string(),
        version: "v1".to_string(),
        priority,
        hash_key: "user_id".to_string(),
        buckets,
        groups,
        enabled: true,
    })
}

fn benchmark_merge_single_layer(c: &mut Criterion) {
    let layer = create_test_layer("test_layer", 100, "test_svc");
    let layers = vec![layer];
    
    let request = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), "user_123".to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
    };
    
    c.bench_function("merge_single_layer", |b| {
        b.iter(|| {
            merge_layers(black_box(&request), black_box(&layers)).unwrap();
        });
    });
}

fn benchmark_merge_multiple_layers(c: &mut Criterion) {
    let layers = vec![
        create_test_layer("layer1", 300, "test_svc"),
        create_test_layer("layer2", 200, "test_svc"),
        create_test_layer("layer3", 100, "test_svc"),
    ];
    
    let request = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), "user_456".to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
    };
    
    c.bench_function("merge_multiple_layers", |b| {
        b.iter(|| {
            merge_layers(black_box(&request), black_box(&layers)).unwrap();
        });
    });
}

fn benchmark_merge_many_layers(c: &mut Criterion) {
    let layers: Vec<_> = (0..10)
        .map(|i| create_test_layer(&format!("layer{}", i), 1000 - i * 100, "test_svc"))
        .collect();
    
    let request = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), "user_789".to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
    };
    
    c.bench_function("merge_many_layers", |b| {
        b.iter(|| {
            merge_layers(black_box(&request), black_box(&layers)).unwrap();
        });
    });
}

criterion_group!(
    benches,
    benchmark_merge_single_layer,
    benchmark_merge_multiple_layers,
    benchmark_merge_many_layers
);
criterion_main!(benches);
