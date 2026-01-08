use crate::error::{ExperimentError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Experiment-level definition (strong cohesion)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentDef {
    /// Globally unique, immutable experiment ID
    pub eid: i64,

    /// Service name (experiment-level shared)
    pub service: String,

    /// Rule (experiment-level shared, evaluated once per request per eid)
    #[serde(default)]
    pub rule: Option<crate::rule::Node>,

    /// Variants under this experiment (only params differ, controlled variable)
    pub variants: Vec<VariantDef>,
}

/// Variant definition within an experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantDef {
    /// Globally unique, immutable variant ID
    pub vid: i64,

    /// JSON or YAML formatted parameters (only this differs across variants in same experiment)
    pub params: serde_json::Value,
}

/// Experiment catalog loaded from `configs/experiments` (or `configs/experiments`)
#[derive(Debug, Clone)]
pub struct ExperimentCatalog {
    /// eid → ExperimentDef
    experiments: HashMap<i64, ExperimentDef>,

    /// vid → eid reverse index (for fast lookup during merge)
    vid_to_eid: HashMap<i64, i64>,

    source_dir: PathBuf,
}

impl ExperimentCatalog {
    pub fn load_from_dir(dir: PathBuf) -> Result<Self> {
        if !dir.exists() {
            tracing::warn!("Experiment catalog directory does not exist: {:?}", dir);
            return Ok(Self {
                experiments: HashMap::new(),
                vid_to_eid: HashMap::new(),
                source_dir: dir,
            });
        }

        let mut experiments: HashMap<i64, ExperimentDef> = HashMap::new();
        let mut vid_to_eid: HashMap<i64, i64> = HashMap::new();

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                continue;
            };

            if ext != "json" && ext != "yaml" && ext != "yml" {
                continue;
            }

            let exp_def = Self::read_experiment_file(&path)?;

            if experiments.contains_key(&exp_def.eid) {
                return Err(ExperimentError::InvalidParameter(format!(
                    "Duplicate eid {} in catalog (file: {:?})",
                    exp_def.eid, path
                )));
            }

            // Build reverse index: vid → eid
            for variant in &exp_def.variants {
                if let Some(existing_eid) = vid_to_eid.insert(variant.vid, exp_def.eid) {
                    return Err(ExperimentError::InvalidParameter(format!(
                        "Duplicate vid {} (belongs to eid {} and {})",
                        variant.vid, existing_eid, exp_def.eid
                    )));
                }
            }

            experiments.insert(exp_def.eid, exp_def);
        }

        Ok(Self {
            experiments,
            vid_to_eid,
            source_dir: dir,
        })
    }

    fn read_experiment_file(path: &Path) -> Result<ExperimentDef> {
        let content = std::fs::read_to_string(path)?;

        // Try JSON first, then YAML
        let def: ExperimentDef = serde_json::from_str(&content)
            .or_else(|_| serde_yaml::from_str(&content).map_err(ExperimentError::from))?;

        Ok(def)
    }

    /// Get experiment by eid
    #[inline]
    pub fn get_experiment(&self, eid: i64) -> Option<&ExperimentDef> {
        self.experiments.get(&eid)
    }

    /// Get eid by vid (reverse index)
    #[inline]
    pub fn get_eid_by_vid(&self, vid: i64) -> Option<i64> {
        self.vid_to_eid.get(&vid).copied()
    }

    /// Get variant params by vid (returns (eid, service, rule, params))
    pub fn get_variant(&self, vid: i64) -> Option<(i64, &str, Option<&crate::rule::Node>, &serde_json::Value)> {
        let eid = self.get_eid_by_vid(vid)?;
        let exp = self.get_experiment(eid)?;
        let variant = exp.variants.iter().find(|v| v.vid == vid)?;
        Some((eid, exp.service.as_str(), exp.rule.as_ref(), &variant.params))
    }

    /// Get all services from catalog (for building inverted index)
    #[allow(dead_code)]
    pub fn get_all_services(&self) -> Vec<String> {
        let mut services: Vec<String> = self
            .experiments
            .values()
            .map(|exp| exp.service.clone())
            .collect();
        services.sort();
        services.dedup();
        services
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.experiments.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.experiments.is_empty()
    }

    #[allow(dead_code)]
    pub fn source_dir(&self) -> &Path {
        &self.source_dir
    }
}
