use crate::config_source::ConfigChange;
use crate::layer::Layer;
use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Implementation details
pub async fn watch_configs(
    layers_dir: PathBuf,
    experiments_dir: PathBuf,
    tx: mpsc::Sender<ConfigChange>,
) -> Result<()> {
    let (event_tx, mut event_rx) = mpsc::channel(100);

    let layers_dir_clone = layers_dir.clone();
    let experiments_dir_clone = experiments_dir.clone();

    // Implementation details
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = event_tx.blocking_send(event);
            }
        },
        Config::default(),
    )?;

    // Implementation details
    if layers_dir.exists() {
        watcher.watch(&layers_dir, RecursiveMode::NonRecursive)?;
        tracing::info!("Watching layers directory: {:?}", layers_dir);
    }

    if experiments_dir.exists() {
        watcher.watch(&experiments_dir, RecursiveMode::NonRecursive)?;
        tracing::info!("Watching experiments directory: {:?}", experiments_dir);
    }

    // Implementation details
    while let Some(event) = event_rx.recv().await {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in event.paths {
                    if let Err(e) =
                        handle_config_change(&layers_dir_clone, &experiments_dir_clone, &path, &tx)
                            .await
                    {
                        tracing::error!("Failed to handle config change {:?}: {}", path, e);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if let Err(e) =
                        handle_config_remove(&layers_dir_clone, &experiments_dir_clone, &path, &tx)
                            .await
                    {
                        tracing::error!("Failed to handle config remove {:?}: {}", path, e);
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}

async fn handle_config_change(
    layers_dir: &Path,
    experiments_dir: &Path,
    path: &Path,
    tx: &mpsc::Sender<ConfigChange>,
) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }

    // Implementation details
    let Some(ext) = path.extension() else {
        return Ok(());
    };

    if ext != "json" && ext != "yaml" && ext != "yml" {
        return Ok(());
    }

    // Implementation details
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Implementation details
    if path.starts_with(layers_dir) {
        match Layer::from_file(path) {
            Ok(layer) => {
                tracing::info!("Detected layer change: {}", layer.layer_id);
                let _ = tx.send(ConfigChange::LayerUpdate { layer }).await;
            }
            Err(e) => {
                tracing::error!("Failed to load layer from {:?}: {}", path, e);
            }
        }
    } else if path.starts_with(experiments_dir) {
        match load_experiment_file(path) {
            Ok(experiment) => {
                tracing::info!("Detected experiment change: eid={}", experiment.eid);
                let _ = tx.send(ConfigChange::ExperimentUpdate { experiment }).await;
            }
            Err(e) => {
                tracing::error!("Failed to load experiment from {:?}: {}", path, e);
            }
        }
    }

    Ok(())
}

async fn handle_config_remove(
    layers_dir: &Path,
    experiments_dir: &Path,
    path: &Path,
    tx: &mpsc::Sender<ConfigChange>,
) -> Result<()> {
    let Some(file_stem) = path.file_stem() else {
        return Ok(());
    };

    if path.starts_with(layers_dir) {
        let layer_id = file_stem.to_string_lossy().to_string();
        tracing::info!("Detected layer removal: {}", layer_id);
        let _ = tx.send(ConfigChange::LayerDelete { layer_id }).await;
    } else if path.starts_with(experiments_dir) {
        if let Ok(eid) = file_stem.to_string_lossy().parse::<i64>() {
            tracing::info!("Detected experiment removal: eid={}", eid);
            let _ = tx.send(ConfigChange::ExperimentDelete { eid }).await;
        }
    }

    Ok(())
}

fn load_experiment_file(path: &Path) -> Result<crate::catalog::ExperimentDef> {
    let content = std::fs::read_to_string(path)?;
    let exp_def = serde_json::from_str(&content).or_else(|_| {
        serde_yaml::from_str(&content).map_err(|e| anyhow::anyhow!("YAML parse error: {}", e))
    })?;
    Ok(exp_def)
}
