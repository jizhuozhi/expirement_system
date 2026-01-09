use crate::config::types::AppConfig;
use crate::utils::error::DataPlaneError;
use std::path::PathBuf;
use tokio::fs;

/// 配置源枚举，简化原来的trait抽象
#[derive(Debug, Clone)]
pub enum ConfigSource {
    File {
        layers_dir: PathBuf,
        experiments_dir: PathBuf,
    },
    Grpc {
        addr: String,
        id: String,
    },
    Xds {
        server_uri: String,
        node_id: String,
    },
}

impl ConfigSource {
    /// 加载配置
    pub async fn load_config(&self) -> Result<AppConfig, DataPlaneError> {
        match self {
            ConfigSource::File { layers_dir, experiments_dir } => {
                self.load_from_files(layers_dir, experiments_dir).await
            }
            ConfigSource::Grpc { addr, id } => {
                self.load_from_grpc(addr, id).await
            }
            ConfigSource::Xds { server_uri, node_id } => {
                self.load_from_xds(server_uri, node_id).await
            }
        }
    }

    async fn load_from_files(&self, layers_dir: &PathBuf, experiments_dir: &PathBuf) -> Result<AppConfig, DataPlaneError> {
        // 统一的文件加载逻辑
        let layers = self.load_json_files(layers_dir).await?;
        let experiments = self.load_json_files(experiments_dir).await?;
        
        Ok(AppConfig {
            layers,
            experiments,
            source: self.clone(),
        })
    }

    async fn load_from_grpc(&self, _addr: &str, _id: &str) -> Result<AppConfig, DataPlaneError> {
        // Implementation details
        Ok(AppConfig::default())
    }

    async fn load_from_xds(&self, _server_uri: &str, _node_id: &str) -> Result<AppConfig, DataPlaneError> {
        // Implementation details
        Ok(AppConfig::default())
    }

    /// 统一的文件加载函数，减少重复代码
    async fn load_json_files(&self, dir: &PathBuf) -> Result<Vec<serde_json::Value>, DataPlaneError> {
        let mut files = Vec::new();
        
        if !dir.exists() {
            return Ok(files);
        }

        let mut entries = fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let content = fs::read_to_string(&path).await?;
                let value: serde_json::Value = serde_json::from_str(&content)?;
                files.push(value);
            }
        }

        Ok(files)
    }
}