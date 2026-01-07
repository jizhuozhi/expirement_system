use experiment_data_plane::error::Result;
use experiment_data_plane::hash::hash_to_bucket_with_salt;
use experiment_data_plane::layer::{Group, Layer};
use experiment_data_plane::merge::{merge_layers, ExperimentRequest};
use experiment_data_plane::rule::{FieldType, Node, Op};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

fn setup_field_types() -> HashMap<String, FieldType> {
    [
        ("user_id".to_string(), FieldType::String),
        ("country".to_string(), FieldType::String),
        ("age".to_string(), FieldType::Int),
        ("balance".to_string(), FieldType::Float),
        ("premium".to_string(), FieldType::Bool),
        ("app_version".to_string(), FieldType::SemVer),
    ]
    .into_iter()
    .collect()
}

#[test]
fn test_rule_validation_and_evaluation() -> Result<()> {
    let field_types = setup_field_types();
    
    // Create a rule: country == "US" AND age >= 18
    let rule = Node::And {
        children: vec![
            Node::Field {
                field: "country".to_string(),
                op: Op::Eq,
                values: vec![json!("US")],
            },
            Node::Field {
                field: "age".to_string(),
                op: Op::Gte,
                values: vec![json!(18)],
            },
        ],
    };
    
    // Validate rule
    rule.validate(&field_types)?;
    
    // Test evaluation - should pass
    let ctx_pass = [
        ("country".to_string(), json!("US")),
        ("age".to_string(), json!(25)),
    ]
    .into_iter()
    .collect();
    
    assert_eq!(rule.evaluate(&ctx_pass, &field_types)?, true);
    
    // Test evaluation - should fail (country mismatch)
    let ctx_fail = [
        ("country".to_string(), json!("CN")),
        ("age".to_string(), json!(25)),
    ]
    .into_iter()
    .collect();
    
    assert_eq!(rule.evaluate(&ctx_fail, &field_types)?, false);
    
    Ok(())
}

#[test]
fn test_layer_merge_with_rules() -> Result<()> {
    let field_types = setup_field_types();
    let test_user = "user_rule_test";
    let layer_salt = "rule_layer_v1";
    let bucket = hash_to_bucket_with_salt(test_user, layer_salt);
    
    // Create a rule: country == "US" AND age >= 18
    let rule = Node::And {
        children: vec![
            Node::Field {
                field: "country".to_string(),
                op: Op::Eq,
                values: vec![json!("US")],
            },
            Node::Field {
                field: "age".to_string(),
                op: Op::Gte,
                values: vec![json!(18)],
            },
        ],
    };
    
    let layer = Arc::new(Layer {
        layer_id: "rule_layer".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(layer_salt.to_string()),
        buckets: [(bucket, "group_us_adult".to_string())]
            .into_iter()
            .collect(),
        groups: [(
            "group_us_adult".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"feature_flag": true, "discount": 0.1}),
                rule: Some(rule),
            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });
    
    // Test 1: User matches rule (US, age 25)
    let request_pass = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), test_user.to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: [
            ("country".to_string(), json!("US")),
            ("age".to_string(), json!(25)),
        ]
        .into_iter()
        .collect(),
    };
    
    let response = merge_layers(&request_pass, &[layer.clone()], &field_types)?;
    assert_eq!(response.matched_layers.len(), 1);
    assert_eq!(response.parameters["feature_flag"], json!(true));
    assert_eq!(response.parameters["discount"], json!(0.1));
    
    // Test 2: User doesn't match rule (CN, age 25)
    let request_fail = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), test_user.to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: [
            ("country".to_string(), json!("CN")),
            ("age".to_string(), json!(25)),
        ]
        .into_iter()
        .collect(),
    };
    
    let response = merge_layers(&request_fail, &[layer.clone()], &field_types)?;
    assert_eq!(response.matched_layers.len(), 0);
    assert!(response.parameters.as_object().unwrap().is_empty());
    
    // Test 3: User doesn't match rule (US, age 16)
    let request_fail2 = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), test_user.to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: [
            ("country".to_string(), json!("US")),
            ("age".to_string(), json!(16)),
        ]
        .into_iter()
        .collect(),
    };
    
    let response = merge_layers(&request_fail2, &[layer], &field_types)?;
    assert_eq!(response.matched_layers.len(), 0);
    
    Ok(())
}

#[test]
fn test_complex_rule_scenarios() -> Result<()> {
    let field_types = setup_field_types();
    
    // Rule: (country == "US" OR country == "CA") AND (age >= 18 OR premium == true)
    let rule = Node::And {
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
            Node::Or {
                children: vec![
                    Node::Field {
                        field: "age".to_string(),
                        op: Op::Gte,
                        values: vec![json!(18)],
                    },
                    Node::Field {
                        field: "premium".to_string(),
                        op: Op::Eq,
                        values: vec![json!(true)],
                    },
                ],
            },
        ],
    };
    
    rule.validate(&field_types)?;
    
    // Test case 1: US, age 25, not premium - should pass
    let ctx1 = [
        ("country".to_string(), json!("US")),
        ("age".to_string(), json!(25)),
        ("premium".to_string(), json!(false)),
    ]
    .into_iter()
    .collect();
    assert_eq!(rule.evaluate(&ctx1, &field_types)?, true);
    
    // Test case 2: CA, age 16, premium - should pass
    let ctx2 = [
        ("country".to_string(), json!("CA")),
        ("age".to_string(), json!(16)),
        ("premium".to_string(), json!(true)),
    ]
    .into_iter()
    .collect();
    assert_eq!(rule.evaluate(&ctx2, &field_types)?, true);
    
    // Test case 3: UK, age 25, not premium - should fail
    let ctx3 = [
        ("country".to_string(), json!("UK")),
        ("age".to_string(), json!(25)),
        ("premium".to_string(), json!(false)),
    ]
    .into_iter()
    .collect();
    assert_eq!(rule.evaluate(&ctx3, &field_types)?, false);
    
    // Test case 4: US, age 16, not premium - should fail
    let ctx4 = [
        ("country".to_string(), json!("US")),
        ("age".to_string(), json!(16)),
        ("premium".to_string(), json!(false)),
    ]
    .into_iter()
    .collect();
    assert_eq!(rule.evaluate(&ctx4, &field_types)?, false);
    
    Ok(())
}

#[test]
fn test_in_operator() -> Result<()> {
    let field_types = setup_field_types();
    
    // Rule: country IN ["US", "CA", "UK"]
    let rule = Node::Field {
        field: "country".to_string(),
        op: Op::In,
        values: vec![json!("US"), json!("CA"), json!("UK")],
    };
    
    rule.validate(&field_types)?;
    
    let ctx_us = [("country".to_string(), json!("US"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_us, &field_types)?, true);
    
    let ctx_cn = [("country".to_string(), json!("CN"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_cn, &field_types)?, false);
    
    Ok(())
}

#[test]
fn test_like_operator() -> Result<()> {
    let field_types = setup_field_types();
    
    // Rule: user_id LIKE "test_*"
    let rule = Node::Field {
        field: "user_id".to_string(),
        op: Op::Like,
        values: vec![json!("test_*")],
    };
    
    rule.validate(&field_types)?;
    
    let ctx_match = [("user_id".to_string(), json!("test_12345"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_match, &field_types)?, true);
    
    let ctx_no_match = [("user_id".to_string(), json!("user_12345"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_no_match, &field_types)?, false);
    
    Ok(())
}

#[test]
fn test_semver_comparison() -> Result<()> {
    let field_types = setup_field_types();
    
    // Rule: app_version >= "2.0.0"
    let rule = Node::Field {
        field: "app_version".to_string(),
        op: Op::Gte,
        values: vec![json!("2.0.0")],
    };
    
    rule.validate(&field_types)?;
    
    let ctx_newer = [("app_version".to_string(), json!("2.5.1"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_newer, &field_types)?, true);
    
    let ctx_older = [("app_version".to_string(), json!("1.9.9"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_older, &field_types)?, false);
    
    Ok(())
}

#[test]
fn test_not_operator() -> Result<()> {
    let field_types = setup_field_types();
    
    // Rule: NOT (country == "US")
    let rule = Node::Not {
        child: Box::new(Node::Field {
            field: "country".to_string(),
            op: Op::Eq,
            values: vec![json!("US")],
        }),
    };
    
    rule.validate(&field_types)?;
    
    let ctx_us = [("country".to_string(), json!("US"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_us, &field_types)?, false);
    
    let ctx_cn = [("country".to_string(), json!("CN"))]
        .into_iter()
        .collect();
    assert_eq!(rule.evaluate(&ctx_cn, &field_types)?, true);
    
    Ok(())
}

#[test]
fn test_multiple_layers_with_different_rules() -> Result<()> {
    let field_types = setup_field_types();
    let test_user = "user_multi_rule";
    
    let salt1 = "multi_layer1_v1";
    let salt2 = "multi_layer2_v1";
    let bucket1 = hash_to_bucket_with_salt(test_user, salt1);
    let bucket2 = hash_to_bucket_with_salt(test_user, salt2);
    
    // Layer 1: Rule for US users
    let rule1 = Node::Field {
        field: "country".to_string(),
        op: Op::Eq,
        values: vec![json!("US")],
    };
    
    let layer1 = Arc::new(Layer {
        layer_id: "layer1".to_string(),
        version: "v1".to_string(),
        priority: 200,
        hash_key: "user_id".to_string(),
        salt: Some(salt1.to_string()),
        buckets: [(bucket1, "group_us".to_string())].into_iter().collect(),
        groups: [(
            "group_us".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"region": "US", "timeout": 100}),
                rule: Some(rule1),
            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });
    
    // Layer 2: Rule for premium users
    let rule2 = Node::Field {
        field: "premium".to_string(),
        op: Op::Eq,
        values: vec![json!(true)],
    };
    
    let layer2 = Arc::new(Layer {
        layer_id: "layer2".to_string(),
        version: "v1".to_string(),
        priority: 100,
        hash_key: "user_id".to_string(),
        salt: Some(salt2.to_string()),
        buckets: [(bucket2, "group_premium".to_string())]
            .into_iter()
            .collect(),
        groups: [(
            "group_premium".to_string(),
            Group {
                service: "test_svc".to_string(),
                params: json!({"premium_feature": true, "timeout": 200}),
                rule: Some(rule2),
            },
        )]
        .into_iter()
        .collect(),
        enabled: true,
    });
    
    // Test: US premium user - should match both layers
    let request = ExperimentRequest {
        service: "test_svc".to_string(),
        hash_keys: [("user_id".to_string(), test_user.to_string())]
            .into_iter()
            .collect(),
        layers: vec![],
        context: [
            ("country".to_string(), json!("US")),
            ("premium".to_string(), json!(true)),
        ]
        .into_iter()
        .collect(),
    };
    
    let response = merge_layers(&request, &[layer1, layer2], &field_types)?;
    assert_eq!(response.matched_layers.len(), 2);
    assert_eq!(response.parameters["region"], json!("US"));
    assert_eq!(response.parameters["premium_feature"], json!(true));
    // Layer1 has higher priority, so its timeout wins
    assert_eq!(response.parameters["timeout"], json!(100));
    
    Ok(())
}
