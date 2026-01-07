use crate::layer::LayerManager;
use anyhow::Result;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Watch layers directory for changes and hot reload
pub async fn watch_layers(manager: Arc<LayerManager>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(100);
    
    let layers_dir = manager.layers_dir.clone();
    
    // Create watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        Config::default(),
    )?;
    
    // Watch the layers directory
    watcher.watch(&layers_dir, RecursiveMode::NonRecursive)?;
    
    tracing::info!("Watching layers directory: {:?}", layers_dir);
    
    // Process events
    while let Some(event) = rx.recv().await {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in event.paths {
                    if let Err(e) = handle_file_change(&manager, &path).await {
                        tracing::error!("Failed to handle file change {:?}: {}", path, e);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if let Err(e) = handle_file_remove(&manager, &path).await {
                        tracing::error!("Failed to handle file remove {:?}: {}", path, e);
                    }
                }
            }
            _ => {}
        }
    }
    
    Ok(())
}

async fn handle_file_change(manager: &LayerManager, path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    
    // Check file extension
    if let Some(ext) = path.extension() {
        if ext == "json" || ext == "yaml" || ext == "yml" {
            // Extract layer_id from filename (without extension)
            if let Some(file_stem) = path.file_stem() {
                let layer_id = file_stem.to_string_lossy();
                
                tracing::info!("Detected change in layer file: {:?}", path);
                
                // Add small delay to ensure file write is complete
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                match manager.load_layer(&layer_id, path).await {
                    Ok(_) => {
                        tracing::info!("Hot reloaded layer: {}", layer_id);
                        crate::metrics::LAYER_RELOAD_TOTAL.inc();
                    }
                    Err(e) => {
                        tracing::error!("Failed to reload layer {}: {}", layer_id, e);
                        crate::metrics::LAYER_RELOAD_ERRORS.inc();
                    }
                }
            }
        }
    }
    
    Ok(())
}

async fn handle_file_remove(manager: &LayerManager, path: &Path) -> Result<()> {
    if let Some(file_stem) = path.file_stem() {
        let layer_id = file_stem.to_string_lossy();
        
        tracing::info!("Detected removal of layer file: {:?}", path);
        
        if let Err(e) = manager.remove_layer(&layer_id).await {
            tracing::error!("Failed to remove layer {}: {}", layer_id, e);
        } else {
            tracing::info!("Removed layer: {}", layer_id);
        }
    }
    
    Ok(())
}
