use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use experiment_data_plane::catalog::{ExperimentCatalog, ExperimentDef, VariantDef};
use experiment_data_plane::layer::{BucketRange, Layer, LayerManager};
use rand::Rng;
use serde_json::json;
use tempfile::TempDir;

/// Benchmark implementation
fn create_random_catalog(num_experiments: usize) -> (TempDir, ExperimentCatalog) {
    let mut rng = rand::thread_rng();
    let temp_dir = TempDir::new().unwrap();
    let experiments_dir = temp_dir.path().join("experiments");
    std::fs::create_dir_all(&experiments_dir).unwrap();

    for i in 0..num_experiments {
        let exp = ExperimentDef {
            eid: (100 + i) as i64,
            service: format!("service_{}", rng.gen_range(0..10)),
            rule: None,
            variants: vec![VariantDef {
                vid: (1000 + i * 10) as i64,
                params: json!({"feature": i}),
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

/// Benchmark implementation
async fn create_random_layers(
    num_layers: usize,
    catalog: &ExperimentCatalog,
) -> (TempDir, LayerManager) {
    let mut rng = rand::thread_rng();
    let temp_dir = TempDir::new().unwrap();
    let layers_dir = temp_dir.path().join("layers");
    std::fs::create_dir_all(&layers_dir).unwrap();

    for i in 0..num_layers {
        let bucket_start = rng.gen_range(0..9000);
        let bucket_size = rng.gen_range(100..1000);

        let layer = Layer {
            layer_id: format!("layer_{}", i),
            version: "v1".to_string(),
            priority: (1000000 - i * 10) as i32,
            hash_key: "user_id".to_string(),
            salt: Some(format!("salt_{}", rng.gen_range(0..1000))),
            services: vec![],
            ranges: vec![BucketRange {
                start: bucket_start,
                end: (bucket_start + bucket_size).min(10000),
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

/// Benchmark implementation
fn bench_layer_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer_filtering");
    let rt = tokio::runtime::Runtime::new().unwrap();

    for num_layers in [1_000, 5_000, 10_000, 50_000].iter() {
        let (_temp_catalog, catalog) = create_random_catalog(*num_layers);
        let (_temp_layers, manager) = rt.block_on(create_random_layers(*num_layers, &catalog));

        let test_service = "service_0".to_string();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_layers),
            num_layers,
            |b, _| {
                b.iter(|| {
                    let _layers = manager.get_layers_for_service(black_box(&test_service));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark implementation
fn bench_bucket_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("bucket_calculation");
    let mut rng = rand::thread_rng();

    let users: Vec<String> = (0..1000).map(|i| format!("user_{}", i)).collect();

    let salts: Vec<String> = (0..100).map(|i| format!("salt_{}", i)).collect();

    group.bench_function("hash_to_bucket", |b| {
        b.iter(|| {
            let user = &users[rng.gen_range(0..users.len())];
            let salt = &salts[rng.gen_range(0..salts.len())];
            experiment_data_plane::hash::hash_to_bucket(black_box(user), black_box(salt))
        });
    });

    group.finish();
}

/// Benchmark implementation
fn bench_layer_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer_sorting");
    let rt = tokio::runtime::Runtime::new().unwrap();

    for num_layers in [1_000, 10_000, 50_000].iter() {
        let (_temp_catalog, catalog) = create_random_catalog(*num_layers);
        let (_temp_layers, manager) = rt.block_on(create_random_layers(*num_layers, &catalog));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_layers),
            num_layers,
            |b, _| {
                b.iter(|| {
                    // Benchmark implementation
                    let layer_ids = manager.get_layer_ids();
                    black_box(layer_ids.len());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_layer_filtering,
    bench_bucket_calculation,
    bench_layer_sorting,
);
criterion_main!(benches);
