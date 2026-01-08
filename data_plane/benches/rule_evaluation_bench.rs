use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use experiment_data_plane::rule::{FieldType, Node, Op};
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;

/// Create nested rule tree with specified depth
fn create_nested_rule(depth: usize, seed: usize) -> Node {
    if depth == 0 {
        return Node::Field {
            field: format!("field_{}", seed % 20),
            op: Op::Eq,
            values: vec![json!(seed % 100)],
        };
    }

    if seed % 2 == 0 {
        Node::And {
            children: vec![
                create_nested_rule(depth - 1, seed * 2),
                create_nested_rule(depth - 1, seed * 2 + 1),
                create_nested_rule(depth - 1, seed * 2 + 2),
            ],
        }
    } else {
        Node::Or {
            children: vec![
                create_nested_rule(depth - 1, seed * 3),
                create_nested_rule(depth - 1, seed * 3 + 1),
                create_nested_rule(depth - 1, seed * 3 + 2),
            ],
        }
    }
}

/// Create random context for rule evaluation
fn create_random_context(num_fields: usize) -> HashMap<String, serde_json::Value> {
    let mut rng = rand::thread_rng();
    let mut context = HashMap::new();

    for i in 0..num_fields {
        let value = match rng.gen_range(0..3) {
            0 => json!(rng.gen_range(0..100)),
            1 => json!(format!("value_{}", rng.gen_range(0..50))),
            _ => json!(rng.gen_bool(0.5)),
        };
        context.insert(format!("field_{}", i), value);
    }

    context
}

/// Benchmark: Simple rule evaluation
fn bench_simple_rules(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_rules");

    let mut field_types = HashMap::new();
    field_types.insert("age".to_string(), FieldType::Int);
    field_types.insert("country".to_string(), FieldType::String);
    field_types.insert("premium".to_string(), FieldType::Bool);

    let context = [
        ("age".to_string(), json!(25)),
        ("country".to_string(), json!("US")),
        ("premium".to_string(), json!(true)),
    ]
    .into_iter()
    .collect();

    let rules = vec![
        (
            "eq",
            Node::Field {
                field: "country".to_string(),
                op: Op::Eq,
                values: vec![json!("US")],
            },
        ),
        (
            "in",
            Node::Field {
                field: "country".to_string(),
                op: Op::In,
                values: vec![json!("US"), json!("CA"), json!("UK")],
            },
        ),
        (
            "gte",
            Node::Field {
                field: "age".to_string(),
                op: Op::Gte,
                values: vec![json!(18)],
            },
        ),
    ];

    for (name, rule) in rules.iter() {
        group.bench_with_input(BenchmarkId::from_parameter(name), name, |b, _| {
            b.iter(|| {
                rule.evaluate(black_box(&context), black_box(&field_types)).unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark: Rule depth complexity
fn bench_rule_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("rule_depth");
    group.sample_size(50);

    let mut field_types = HashMap::new();
    for i in 0..20 {
        field_types.insert(format!("field_{}", i), FieldType::Int);
    }

    let context = create_random_context(20);

    for depth in [2, 4, 6, 8, 10, 15, 20].iter() {
        let rule = create_nested_rule(*depth, 42);

        group.bench_with_input(
            BenchmarkId::from_parameter(depth),
            depth,
            |b, _| {
                b.iter(|| {
                    rule.evaluate(black_box(&context), black_box(&field_types))
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Rule width (number of conditions at same level)
fn bench_rule_width(c: &mut Criterion) {
    let mut group = c.benchmark_group("rule_width");

    let mut field_types = HashMap::new();
    for i in 0..100 {
        field_types.insert(format!("field_{}", i), FieldType::Int);
    }

    let context = create_random_context(100);

    for width in [5, 10, 20, 50, 100].iter() {
        let children: Vec<Node> = (0..*width)
            .map(|i| Node::Field {
                field: format!("field_{}", i),
                op: Op::Eq,
                values: vec![json!(i * 10)],
            })
            .collect();

        let rule = Node::And { children };

        group.bench_with_input(
            BenchmarkId::from_parameter(width),
            width,
            |b, _| {
                b.iter(|| {
                    rule.evaluate(black_box(&context), black_box(&field_types))
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Complex rule patterns
fn bench_complex_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("complex_patterns");

    let mut field_types = HashMap::new();
    field_types.insert("age".to_string(), FieldType::Int);
    field_types.insert("country".to_string(), FieldType::String);
    field_types.insert("premium".to_string(), FieldType::Bool);
    field_types.insert("score".to_string(), FieldType::Int);

    let context = [
        ("age".to_string(), json!(25)),
        ("country".to_string(), json!("US")),
        ("premium".to_string(), json!(true)),
        ("score".to_string(), json!(85)),
    ]
    .into_iter()
    .collect();

    // Pattern 1: Nested AND/OR
    let pattern1 = Node::And {
        children: vec![
            Node::Or {
                children: vec![
                    Node::Field {
                        field: "country".to_string(),
                        op: Op::Eq,
                        values: vec![json!("US")],
                    },
                    Node::Field {
                        field: "country".to_string(),
                        op: Op::Eq,
                        values: vec![json!("CA")],
                    },
                ],
            },
            Node::Field {
                field: "age".to_string(),
                op: Op::Gte,
                values: vec![json!(18)],
            },
        ],
    };

    // Pattern 2: Complex nested
    let pattern2 = Node::And {
        children: vec![
            Node::Or {
                children: vec![
                    Node::And {
                        children: vec![
                            Node::Field {
                                field: "country".to_string(),
                                op: Op::In,
                                values: vec![json!("US"), json!("CA"), json!("UK")],
                            },
                            Node::Field {
                                field: "age".to_string(),
                                op: Op::Gte,
                                values: vec![json!(18)],
                            },
                        ],
                    },
                    Node::Field {
                        field: "premium".to_string(),
                        op: Op::Eq,
                        values: vec![json!(true)],
                    },
                ],
            },
            Node::Field {
                field: "score".to_string(),
                op: Op::Gt,
                values: vec![json!(70)],
            },
        ],
    };

    let patterns = vec![("nested_and_or", pattern1), ("complex_nested", pattern2)];

    for (name, rule) in patterns.iter() {
        group.bench_with_input(BenchmarkId::from_parameter(name), name, |b, _| {
            b.iter(|| {
                rule.evaluate(black_box(&context), black_box(&field_types)).unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark: Batch rule evaluation (multiple rules)
fn bench_batch_evaluation(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_evaluation");
    group.sample_size(50);

    let mut field_types = HashMap::new();
    for i in 0..20 {
        field_types.insert(format!("field_{}", i), FieldType::Int);
    }

    let context = create_random_context(20);

    for num_rules in [10, 50, 100, 500, 1_000, 5_000].iter() {
        let rules: Vec<Node> = (0..*num_rules)
            .map(|i| create_nested_rule(3, i))
            .collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(num_rules),
            num_rules,
            |b, _| {
                b.iter(|| {
                    for rule in &rules {
                        rule.evaluate(black_box(&context), black_box(&field_types))
                            .ok();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_rules,
    bench_rule_depth,
    bench_rule_width,
    bench_complex_patterns,
    bench_batch_evaluation,
);
criterion_main!(benches);
