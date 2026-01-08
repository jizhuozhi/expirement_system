use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use experiment_data_plane::catalog::{ExperimentCatalog, ExperimentDef, VariantDef};
use experiment_data_plane::layer::{BucketRange, Layer, LayerManager};
use experiment_data_plane::merge::{merge_layers_batch, ExperimentRequest};
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

/// Create random nested params with specified depth and width
fn create_random_nested_params(depth: usize, fields_per_level: usize, seed: usize) -> serde_json::Value {
    let mut rng = rand::thread_rng();
    
    if depth == 0 {
        return match rng.gen_range(0..3) {
            0 => json!(rng.gen_range(0..1000)),
            1 => json!(format!("value_{}_{}", seed, rng.gen_range(0..100))),
            _ => json!(rng.gen_bool(0.5)),
        };
    }

    let mut obj = serde_json::Map::new();
    for i in 0..fields_per_level {
        let key = format!("field_{}_{}", depth, i);
        obj.insert(key, create_random_nested_params(depth - 1, fields_per_level, seed * 10 + i));
    }
    
    json!(obj)
}

/// Create catalog with random params
fn create_catalog_with_random_params(
    num_experiments: usize,
    param_depth: usize,
    fields_per_level: usize,
) -> (TempDir, ExperimentCatalog) {
    let temp_dir = TempDir::new().unwrap();
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&experiments_dir).unwrap();

    for i in 0..num_experiments {
        let params = create_random_nested_params(param_depth, fields_per_level, i);
        
        let exp = ExperimentDef {
            eid: (100 + i) as i64,
            service: "test_service".to_string(),
            rule: None,
            variants: vec![VariantDef {
                vid: (1000 + i * 10) as i64,
                params,
            }],
        };

        std::fs::write(
            experiments_dir.join(format!("{}.json", 100 + i)),
            serde_json::to_string_pretty(&exp).unwrap(),
        )
        .unwrap();
    }

    let catalog = ExperimentCatalog::load_from_dir(experiments_dir).unwrap();
    (temp_dir, catalog)
}

/// Create layers
async fn create_layers(num_layers: usize, catalog: &ExperimentCatalog) -> (TempDir, LayerManager) {
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    std::fs::create_dir_all(&layers_dir).unwrap();

    let test_user = "bench_user";
    for i in 0..num_layers {
        let salt = format!("salt_{}", i);
        let bucket = experiment_data_plane::hash::hash_to_bucket(test_user, &salt);

        let layer = Layer {
            layer_id: format!("layer_{}", i),
            version: "v1".to_string(),
            priority: (1000000 - i * 10) as i32,
            hash_key: "user_id".to_string(),
            salt: Some(salt),
            services: vec![],
            ranges: vec![BucketRange {
                start: bucket,
                end: bucket.saturating_add(1).min(10000),
                vid: (1000 + i * 10) as i64,
            }],
            enabled: true,
        };

        std::fs::write(
            layers_dir.join(format!("layer_{}.json", i)),
            serde_json::to_string_pretty(&layer).unwrap(),
        )
        .unwrap();
    }

    let manager = LayerManager::new(layers_dir);
    manager.load_all_layers(catalog).await.unwrap();

    (temp_dir, manager)
}

/// Benchmark: Merge with increasing layers
fn bench_merge_layer_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_layer_count");
    group.sample_size(50);
    let rt = tokio::runtime::Runtime::new().unwrap();

    for num_layers in [10, 50, 100, 500, 1_000, 5_000, 10_000].iter() {
        let (_temp_catalog, catalog) = create_catalog_with_random_params(*num_layers, 3, 5);
        let (_temp_layers, manager) = rt.block_on(create_layers(*num_layers, &catalog));

        let request = ExperimentRequest {
            services: vec!["test_service".to_string()],
            context: [("user_id".to_string(), json!("bench_user"))]
                .into_iter()
                .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_layers),
            num_layers,
            |b, _| {
                b.iter(|| {
                    merge_layers_batch(
                        black_box(&request),
                        black_box(&manager),
                        black_box(&catalog),
                        black_box(&field_types),
                    )
                    .unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Merge with increasing param depth
fn bench_merge_param_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_param_depth");
    group.sample_size(50);
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 深度大时减少宽度，避免指数爆炸
    let test_cases = vec![
        (1, 5),   // 5 fields
        (2, 5),   // 30 fields
        (3, 5),   // 155 fields
        (5, 3),   // 363 fields (改为3宽度)
        (8, 2),   // 510 fields (改为2宽度)
    ];

    for (depth, width) in test_cases.iter() {
        let (_temp_catalog, catalog) = create_catalog_with_random_params(100, *depth, *width);
        let (_temp_layers, manager) = rt.block_on(create_layers(100, &catalog));

        let request = ExperimentRequest {
            services: vec!["test_service".to_string()],
            context: [("user_id".to_string(), json!("bench_user"))]
                .into_iter()
                .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("d{}_w{}", depth, width)),
            &(depth, width),
            |b, _| {
                b.iter(|| {
                    merge_layers_batch(
                        black_box(&request),
                        black_box(&manager),
                        black_box(&catalog),
                        black_box(&field_types),
                    )
                    .unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Merge with increasing field width
fn bench_merge_param_width(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_param_width");
    group.sample_size(50);
    let rt = tokio::runtime::Runtime::new().unwrap();

    for width in [5, 10, 20, 50, 100].iter() {
        let (_temp_catalog, catalog) = create_catalog_with_random_params(100, 3, *width);
        let (_temp_layers, manager) = rt.block_on(create_layers(100, &catalog));

        let request = ExperimentRequest {
            services: vec!["test_service".to_string()],
            context: [("user_id".to_string(), json!("bench_user"))]
                .into_iter()
                .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();

        group.bench_with_input(
            BenchmarkId::from_parameter(width),
            width,
            |b, _| {
                b.iter(|| {
                    merge_layers_batch(
                        black_box(&request),
                        black_box(&manager),
                        black_box(&catalog),
                        black_box(&field_types),
                    )
                    .unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Extreme param merge (combined stress)
fn bench_extreme_param_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("extreme_param_merge");
    group.sample_size(20);
    let rt = tokio::runtime::Runtime::new().unwrap();

    let test_cases = vec![
        ("small", 10, 2, 5),
        ("medium", 50, 3, 10),
        ("large", 100, 4, 15),
        ("huge", 500, 5, 20),
        ("massive", 1_000, 4, 25),
        ("extreme", 5_000, 3, 20),
    ];

    for (label, num_layers, depth, width) in test_cases.iter() {
        let (_temp_catalog, catalog) = create_catalog_with_random_params(*num_layers, *depth, *width);
        let (_temp_layers, manager) = rt.block_on(create_layers(*num_layers, &catalog));

        let request = ExperimentRequest {
            services: vec!["test_service".to_string()],
            context: [("user_id".to_string(), json!("bench_user"))]
                .into_iter()
                .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();

        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            label,
            |b, _| {
                b.iter(|| {
                    merge_layers_batch(
                        black_box(&request),
                        black_box(&manager),
                        black_box(&catalog),
                        black_box(&field_types),
                    )
                    .unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Merge with conflicting keys (override scenarios)
fn bench_merge_conflicts(c: &mut Criterion) {
    let mut group = c.benchmark_group("merge_conflicts");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Create catalog where all params have overlapping keys
    let temp_dir = TempDir::new().unwrap();
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&experiments_dir).unwrap();

    for num_layers in [10, 50, 100, 500].iter() {
        for i in 0..*num_layers {
            let params = json!({
                "common_field": i,
                "shared_config": {
                    "timeout": 100 + i,
                    "retry": i % 5,
                },
                "unique_field": format!("value_{}", i),
            });

            let exp = ExperimentDef {
                eid: (100 + i) as i64,
                service: "test_service".to_string(),
                rule: None,
                variants: vec![VariantDef {
                    vid: (1000 + i * 10) as i64,
                    params,
                }],
            };

            std::fs::write(
                experiments_dir.join(format!("{}.json", 100 + i)),
                serde_json::to_string_pretty(&exp).unwrap(),
            )
            .unwrap();
        }

        let catalog = ExperimentCatalog::load_from_dir(experiments_dir.clone()).unwrap();
        let (_temp_layers, manager) = rt.block_on(create_layers(*num_layers, &catalog));

        let request = ExperimentRequest {
            services: vec!["test_service".to_string()],
            context: [("user_id".to_string(), json!("bench_user"))]
                .into_iter()
                .collect(),
            layers: vec![],
        };

        let field_types = HashMap::new();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_layers),
            num_layers,
            |b, _| {
                b.iter(|| {
                    merge_layers_batch(
                        black_box(&request),
                        black_box(&manager),
                        black_box(&catalog),
                        black_box(&field_types),
                    )
                    .unwrap();
                });
            },
        );

        // Clean up for next iteration
        for i in 0..*num_layers {
            std::fs::remove_file(experiments_dir.join(format!("{}.json", 100 + i))).ok();
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_merge_layer_count,
    bench_merge_param_depth,
    bench_merge_param_width,
    bench_extreme_param_merge,
    bench_merge_conflicts,
);
criterion_main!(benches);
