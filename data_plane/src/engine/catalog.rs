use crate::utils::error::{ExperimentError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Implementation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentDef {
    /// Implementation details
    pub eid: i64,

    /// Implementation details
    pub service: String,

    /// Implementation details
    #[serde(default)]
    pub rule: Option<crate::engine::rule::Node>,

    /// Implementation details
    pub variants: Vec<VariantDef>,
}

/// Implementation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantDef {
    /// Implementation details
    pub vid: i64,

    /// Implementation details
    pub params: serde_json::Value,
}

/// Implementation details
#[derive(Debug, Clone)]
pub struct ExperimentCatalog {
    /// Implementation details
    experiments: HashMap<i64, ExperimentDef>,

    /// Implementation details
    vid_to_eid: HashMap<i64, i64>,
}

impl ExperimentCatalog {
    /// Implementation details
    pub fn from_experiments(experiments_vec: Vec<ExperimentDef>) -> Result<Self> {
        let mut experiments: HashMap<i64, ExperimentDef> = HashMap::new();
        let mut vid_to_eid: HashMap<i64, i64> = HashMap::new();

        for exp_def in experiments_vec {
            if experiments.contains_key(&exp_def.eid) {
                return Err(ExperimentError::InvalidParameter(format!(
                    "Duplicate eid {} in catalog",
                    exp_def.eid
                )));
            }

            // Implementation details
            for variant in &exp_def.variants {
                if let Some(existing_eid) = vid_to_eid.insert(variant.vid, exp_def.eid) {
                    return Err(ExperimentError::InvalidParameter(format!(
                        "Duplicate vid {} (belongs to eid {} and {})",
                        variant.vid, existing_eid, exp_def.eid
                    )));
                }
            }

            tracing::info!(
                "Loaded experiment: eid={}, service={}",
                exp_def.eid,
                exp_def.service
            );
            experiments.insert(exp_def.eid, exp_def);
        }

        Ok(Self {
            experiments,
            vid_to_eid,
        })
    }

    /// Implementation details
    pub fn update_experiment(&mut self, exp_def: ExperimentDef) -> Result<()> {
        // Implementation details
        if let Some(old_exp) = self.experiments.get(&exp_def.eid) {
            for variant in &old_exp.variants {
                self.vid_to_eid.remove(&variant.vid);
            }
        }

        // Implementation details
        for variant in &exp_def.variants {
            if let Some(existing_eid) = self.vid_to_eid.get(&variant.vid) {
                if *existing_eid != exp_def.eid {
                    return Err(ExperimentError::InvalidParameter(format!(
                        "Duplicate vid {} (belongs to eid {} and {})",
                        variant.vid, existing_eid, exp_def.eid
                    )));
                }
            }
            self.vid_to_eid.insert(variant.vid, exp_def.eid);
        }

        tracing::info!(
            "Updated experiment: eid={}, service={}",
            exp_def.eid,
            exp_def.service
        );
        self.experiments.insert(exp_def.eid, exp_def);
        Ok(())
    }

    /// Implementation details
    pub fn remove_experiment(&mut self, eid: i64) {
        if let Some(exp) = self.experiments.remove(&eid) {
            for variant in &exp.variants {
                self.vid_to_eid.remove(&variant.vid);
            }
            tracing::info!("Removed experiment: eid={}", eid);
        }
    }

    /// Implementation details
    #[inline]
    pub fn get_experiment(&self, eid: i64) -> Option<&ExperimentDef> {
        self.experiments.get(&eid)
    }

    /// Implementation details
    #[inline]
    pub fn get_eid_by_vid(&self, vid: i64) -> Option<i64> {
        self.vid_to_eid.get(&vid).copied()
    }

    /// Implementation details
    pub fn get_variant(
        &self,
        vid: i64,
    ) -> Option<(i64, &str, Option<&crate::rule::Node>, &serde_json::Value)> {
        let eid = self.get_eid_by_vid(vid)?;
        let exp = self.get_experiment(eid)?;
        let variant = exp.variants.iter().find(|v| v.vid == vid)?;
        Some((
            eid,
            exp.service.as_str(),
            exp.rule.as_ref(),
            &variant.params,
        ))
    }

    /// Implementation details
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

    pub fn len(&self) -> usize {
        self.experiments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.experiments.is_empty()
    }
}
