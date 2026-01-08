use crate::error::{ExperimentError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Field type information from control plane
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    SemVer,
}

/// Operator for rule evaluation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    // Comparison operators
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    
    // Set operators
    In,
    NotIn,
    
    // String operators
    Like,
    NotLike,
    
    // Boolean operators
    And,
    Or,
    Not,
}

/// Rule node for building expression trees
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// Boolean combination node
    And {
        children: Vec<Node>,
    },
    Or {
        children: Vec<Node>,
    },
    Not {
        child: Box<Node>,
    },
    
    /// Field operation node (leaf node)
    Field {
        field: String,
        op: Op,
        values: Vec<serde_json::Value>,
    },
}

impl Node {
    /// Validate node structure against field type map
    #[allow(dead_code)]
    pub fn validate(&self, field_types: &HashMap<String, FieldType>) -> Result<()> {
        match self {
            Node::And { children } => {
                if children.is_empty() {
                    return Err(ExperimentError::InvalidRule(
                        "And node must have at least one child".to_string()
                    ));
                }
                for child in children {
                    child.validate(field_types)?;
                }
            }
            Node::Or { children } => {
                if children.is_empty() {
                    return Err(ExperimentError::InvalidRule(
                        "Or node must have at least one child".to_string()
                    ));
                }
                for child in children {
                    child.validate(field_types)?;
                }
            }
            Node::Not { child } => {
                child.validate(field_types)?;
            }
            Node::Field { field, op, values } => {
                // Check field exists
                let field_type = field_types
                    .get(field)
                    .ok_or_else(|| ExperimentError::InvalidRule(
                        format!("Field '{}' not found in field type map", field)
                    ))?;
                
                // Check values not empty
                if values.is_empty() {
                    return Err(ExperimentError::InvalidRule(
                        format!("Field '{}' operator {:?} requires at least one value", field, op)
                    ));
                }
                
                // Validate operator is appropriate for boolean nodes
                match op {
                    Op::And | Op::Or | Op::Not => {
                        return Err(ExperimentError::InvalidRule(
                            format!("Boolean operator {:?} cannot be used in Field node", op)
                        ));
                    }
                    _ => {}
                }
                
                // Validate value types match field type
                for value in values {
                    validate_value_type(value, field_type, field)?;
                }
            }
        }
        Ok(())
    }
    
    /// Evaluate node against context
    pub fn evaluate(
        &self,
        ctx: &HashMap<String, serde_json::Value>,
        field_types: &HashMap<String, FieldType>,
    ) -> Result<bool> {
        match self {
            Node::And { children } => {
                for child in children {
                    if !child.evaluate(ctx, field_types)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Node::Or { children } => {
                for child in children {
                    if child.evaluate(ctx, field_types)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Node::Not { child } => {
                let result = child.evaluate(ctx, field_types)?;
                Ok(!result)
            }
            Node::Field { field, op, values } => {
                // Get field value from context
                let field_value = ctx
                    .get(field)
                    .ok_or_else(|| ExperimentError::InvalidRule(
                        format!("Field '{}' not found in context", field)
                    ))?;
                
                // Get field type
                let field_type = field_types
                    .get(field)
                    .ok_or_else(|| ExperimentError::InvalidRule(
                        format!("Field '{}' not found in field type map", field)
                    ))?;
                
                // Evaluate based on operator
                evaluate_field_op(field_value, op, values, field_type)
            }
        }
    }
}

/// Validate that a value matches the expected field type
#[allow(dead_code)]
fn validate_value_type(value: &serde_json::Value, field_type: &FieldType, field_name: &str) -> Result<()> {
    use serde_json::Value;
    
    match (field_type, value) {
        (FieldType::String, Value::String(_)) => Ok(()),
        (FieldType::Int, Value::Number(n)) if n.is_i64() => Ok(()),
        (FieldType::Float, Value::Number(_)) => Ok(()),
        (FieldType::Bool, Value::Bool(_)) => Ok(()),
        (FieldType::SemVer, Value::String(s)) => {
            // Basic semver validation
            if s.split('.').count() >= 2 {
                Ok(())
            } else {
                Err(ExperimentError::InvalidRule(
                    format!("Field '{}' value '{}' is not a valid semver", field_name, s)
                ))
            }
        }
        _ => Err(ExperimentError::InvalidRule(
            format!("Field '{}' value {:?} does not match type {:?}", field_name, value, field_type)
        )),
    }
}

/// Evaluate field operation
fn evaluate_field_op(
    field_value: &serde_json::Value,
    op: &Op,
    values: &[serde_json::Value],
    field_type: &FieldType,
) -> Result<bool> {
    use serde_json::Value;
    
    match op {
        Op::Eq => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Eq operator requires exactly one value".to_string()
                ));
            }
            Ok(compare_values(field_value, &values[0], field_type)? == std::cmp::Ordering::Equal)
        }
        Op::Neq => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Neq operator requires exactly one value".to_string()
                ));
            }
            Ok(compare_values(field_value, &values[0], field_type)? != std::cmp::Ordering::Equal)
        }
        Op::Gt => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Gt operator requires exactly one value".to_string()
                ));
            }
            Ok(compare_values(field_value, &values[0], field_type)? == std::cmp::Ordering::Greater)
        }
        Op::Gte => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Gte operator requires exactly one value".to_string()
                ));
            }
            let cmp = compare_values(field_value, &values[0], field_type)?;
            Ok(cmp == std::cmp::Ordering::Greater || cmp == std::cmp::Ordering::Equal)
        }
        Op::Lt => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Lt operator requires exactly one value".to_string()
                ));
            }
            Ok(compare_values(field_value, &values[0], field_type)? == std::cmp::Ordering::Less)
        }
        Op::Lte => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Lte operator requires exactly one value".to_string()
                ));
            }
            let cmp = compare_values(field_value, &values[0], field_type)?;
            Ok(cmp == std::cmp::Ordering::Less || cmp == std::cmp::Ordering::Equal)
        }
        Op::In => {
            for value in values {
                if compare_values(field_value, value, field_type)? == std::cmp::Ordering::Equal {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Op::NotIn => {
            for value in values {
                if compare_values(field_value, value, field_type)? == std::cmp::Ordering::Equal {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Op::Like => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "Like operator requires exactly one value".to_string()
                ));
            }
            match (field_value, &values[0]) {
                (Value::String(field_str), Value::String(pattern)) => {
                    // Simple pattern matching: * as wildcard
                    Ok(simple_pattern_match(field_str, pattern))
                }
                _ => Err(ExperimentError::InvalidRule(
                    "Like operator requires string values".to_string()
                )),
            }
        }
        Op::NotLike => {
            if values.len() != 1 {
                return Err(ExperimentError::InvalidRule(
                    "NotLike operator requires exactly one value".to_string()
                ));
            }
            match (field_value, &values[0]) {
                (Value::String(field_str), Value::String(pattern)) => {
                    Ok(!simple_pattern_match(field_str, pattern))
                }
                _ => Err(ExperimentError::InvalidRule(
                    "NotLike operator requires string values".to_string()
                )),
            }
        }
        Op::And | Op::Or | Op::Not => {
            Err(ExperimentError::InvalidRule(
                format!("Boolean operator {:?} cannot be used in field comparison", op)
            ))
        }
    }
}

/// Compare two values based on field type
fn compare_values(
    left: &serde_json::Value,
    right: &serde_json::Value,
    field_type: &FieldType,
) -> Result<std::cmp::Ordering> {
    use serde_json::Value;
    
    match field_type {
        FieldType::String => {
            match (left, right) {
                (Value::String(l), Value::String(r)) => Ok(l.cmp(r)),
                _ => Err(ExperimentError::InvalidRule(
                    "String comparison requires string values".to_string()
                )),
            }
        }
        FieldType::Int => {
            match (left.as_i64(), right.as_i64()) {
                (Some(l), Some(r)) => Ok(l.cmp(&r)),
                _ => Err(ExperimentError::InvalidRule(
                    "Int comparison requires integer values".to_string()
                )),
            }
        }
        FieldType::Float => {
            match (left.as_f64(), right.as_f64()) {
                (Some(l), Some(r)) => {
                    if l < r {
                        Ok(std::cmp::Ordering::Less)
                    } else if l > r {
                        Ok(std::cmp::Ordering::Greater)
                    } else {
                        Ok(std::cmp::Ordering::Equal)
                    }
                }
                _ => Err(ExperimentError::InvalidRule(
                    "Float comparison requires numeric values".to_string()
                )),
            }
        }
        FieldType::Bool => {
            match (left.as_bool(), right.as_bool()) {
                (Some(l), Some(r)) => Ok(l.cmp(&r)),
                _ => Err(ExperimentError::InvalidRule(
                    "Bool comparison requires boolean values".to_string()
                )),
            }
        }
        FieldType::SemVer => {
            match (left.as_str(), right.as_str()) {
                (Some(l), Some(r)) => compare_semver(l, r),
                _ => Err(ExperimentError::InvalidRule(
                    "SemVer comparison requires string values".to_string()
                )),
            }
        }
    }
}

/// Compare semantic versions
fn compare_semver(left: &str, right: &str) -> Result<std::cmp::Ordering> {
    let left_parts: Vec<u32> = left
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    let right_parts: Vec<u32> = right
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    
    if left_parts.is_empty() || right_parts.is_empty() {
        return Err(ExperimentError::InvalidRule(
            format!("Invalid semver format: {} or {}", left, right)
        ));
    }
    
    Ok(left_parts.cmp(&right_parts))
}

/// Simple pattern matching with * wildcard
fn simple_pattern_match(text: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    
    if !pattern.contains('*') {
        return text == pattern;
    }
    
    let parts: Vec<&str> = pattern.split('*').collect();
    
    // Pattern like "*suffix"
    if pattern.starts_with('*') && parts.len() == 2 {
        return text.ends_with(parts[1]);
    }
    
    // Pattern like "prefix*"
    if pattern.ends_with('*') && parts.len() == 2 {
        return text.starts_with(parts[0]);
    }
    
    // Pattern like "prefix*suffix"
    if parts.len() == 2 {
        return text.starts_with(parts[0]) && text.ends_with(parts[1]);
    }
    
    // More complex patterns - fall back to simple check
    text.contains(parts[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
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
    fn test_node_validation_success() {
        let field_types = setup_field_types();
        
        let node = Node::And {
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
        
        assert!(node.validate(&field_types).is_ok());
    }
    
    #[test]
    fn test_node_validation_field_not_found() {
        let field_types = setup_field_types();
        
        let node = Node::Field {
            field: "unknown_field".to_string(),
            op: Op::Eq,
            values: vec![json!("value")],
        };
        
        assert!(node.validate(&field_types).is_err());
    }
    
    #[test]
    fn test_node_validation_empty_values() {
        let field_types = setup_field_types();
        
        let node = Node::Field {
            field: "country".to_string(),
            op: Op::Eq,
            values: vec![],
        };
        
        assert!(node.validate(&field_types).is_err());
    }
    
    #[test]
    fn test_node_validation_type_mismatch() {
        let field_types = setup_field_types();
        
        let node = Node::Field {
            field: "age".to_string(),
            op: Op::Eq,
            values: vec![json!("not_a_number")],
        };
        
        assert!(node.validate(&field_types).is_err());
    }
    
    #[test]
    fn test_evaluate_eq() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("US")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "country".to_string(),
            op: Op::Eq,
            values: vec![json!("US")],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_neq() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("CN")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "country".to_string(),
            op: Op::Neq,
            values: vec![json!("US")],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_gte() {
        let field_types = setup_field_types();
        let ctx = [
            ("age".to_string(), json!(25)),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "age".to_string(),
            op: Op::Gte,
            values: vec![json!(18)],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_in() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("US")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "country".to_string(),
            op: Op::In,
            values: vec![json!("US"), json!("CA"), json!("UK")],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_not_in() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("CN")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "country".to_string(),
            op: Op::NotIn,
            values: vec![json!("US"), json!("CA"), json!("UK")],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_like() {
        let field_types = setup_field_types();
        let ctx = [
            ("user_id".to_string(), json!("user_12345")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Field {
            field: "user_id".to_string(),
            op: Op::Like,
            values: vec![json!("user_*")],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_and() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("US")),
            ("age".to_string(), json!(25)),
        ]
        .into_iter()
        .collect();
        
        let node = Node::And {
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
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_or() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("CN")),
            ("age".to_string(), json!(25)),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Or {
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
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_not() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("CN")),
        ]
        .into_iter()
        .collect();
        
        let node = Node::Not {
            child: Box::new(Node::Field {
                field: "country".to_string(),
                op: Op::Eq,
                values: vec![json!("US")],
            }),
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_evaluate_nested() {
        let field_types = setup_field_types();
        let ctx = [
            ("country".to_string(), json!("US")),
            ("age".to_string(), json!(25)),
            ("premium".to_string(), json!(true)),
        ]
        .into_iter()
        .collect();
        
        // (country == "US" AND age >= 18) OR premium == true
        let node = Node::Or {
            children: vec![
                Node::And {
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
                },
                Node::Field {
                    field: "premium".to_string(),
                    op: Op::Eq,
                    values: vec![json!(true)],
                },
            ],
        };
        
        assert_eq!(node.evaluate(&ctx, &field_types).unwrap(), true);
    }
    
    #[test]
    fn test_compare_semver() {
        assert_eq!(compare_semver("1.2.3", "1.2.3").unwrap(), std::cmp::Ordering::Equal);
        assert_eq!(compare_semver("1.2.4", "1.2.3").unwrap(), std::cmp::Ordering::Greater);
        assert_eq!(compare_semver("1.2.2", "1.2.3").unwrap(), std::cmp::Ordering::Less);
        assert_eq!(compare_semver("2.0.0", "1.9.9").unwrap(), std::cmp::Ordering::Greater);
    }
    
    #[test]
    fn test_simple_pattern_match() {
        assert_eq!(simple_pattern_match("hello", "*"), true);
        assert_eq!(simple_pattern_match("hello", "hello"), true);
        assert_eq!(simple_pattern_match("hello", "world"), false);
        assert_eq!(simple_pattern_match("hello_world", "hello*"), true);
        assert_eq!(simple_pattern_match("hello_world", "*world"), true);
        assert_eq!(simple_pattern_match("hello_world", "hello*world"), true);
        assert_eq!(simple_pattern_match("hello_world", "hi*"), false);
    }
}
