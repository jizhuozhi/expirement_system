use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExperimentError {
    #[error("Layer not found: {0}")]
    LayerNotFound(String),

    #[error("Group not found: {0}")]
    GroupNotFound(String),

    #[error("Invalid parameter format: {0}")]
    InvalidParameter(String),

    #[error("Invalid rule: {0}")]
    InvalidRule(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

// 别名，保持向后兼容
pub type DataPlaneError = ExperimentError;

pub type Result<T> = std::result::Result<T, ExperimentError>;
