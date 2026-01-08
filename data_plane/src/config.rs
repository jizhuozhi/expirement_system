use anyhow::Result;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub layers_dir: PathBuf,
    pub experiments_dir: PathBuf,
    pub server_host: String,
    pub server_port: u16,
    #[allow(dead_code)]
    pub metrics_port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Backward compat: support GROUPS_DIR as fallback
        let experiments_dir = std::env::var("EXPERIMENTS_DIR")
            .or_else(|_| std::env::var("GROUPS_DIR"))
            .unwrap_or_else(|_| "../configs/experiments".to_string())
            .into();

        Ok(Self {
            layers_dir: std::env::var("LAYERS_DIR")
                .unwrap_or_else(|_| "../configs/layers".to_string())
                .into(),
            experiments_dir,
            server_host: std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()?,
            metrics_port: std::env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9090".to_string())
                .parse()?,
        })
    }
}
