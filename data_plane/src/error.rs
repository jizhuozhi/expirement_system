use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExperimentError {
    #[error("Layer not found: {0}")]
    LayerNotFound(String),

    #[error("Invalid layer version: {0}")]
    InvalidVersion(String),

    #[error("Hash key not found in request: {0}")]
    #[allow(dead_code)]
    HashKeyNotFound(String),

    #[error("Bucket not found: {0}")]
    #[allow(dead_code)]
    BucketNotFound(u32),

    #[error("Group not found: {0}")]
    GroupNotFound(String),

    #[error("Service mismatch: expected {expected}, got {actual}")]
    #[allow(dead_code)]
    ServiceMismatch { expected: String, actual: String },

    #[error("Invalid parameter format: {0}")]
    InvalidParameter(String),

    #[error("Invalid rule: {0}")]
    InvalidRule(String),

    #[error("Rule evaluation failed: {0}")]
    #[allow(dead_code)]
    RuleEvaluationFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, ExperimentError>;
