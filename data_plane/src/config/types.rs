use crate::config::sources::ConfigSource;
use serde::{Deserialize, Serialize};

/// 应用配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub layers: Vec<serde_json::Value>,
    pub experiments: Vec<serde_json::Value>,
    pub source: ConfigSource,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            experiments: Vec::new(),
            source: ConfigSource::File {
                layers_dir: "configs/layers".into(),
                experiments_dir: "configs/experiments".into(),
            },
        }
    }
}

/// 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub grpc_port: Option<u16>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            grpc_port: Some(9090),
        }
    }
}