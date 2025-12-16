use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Defines the traffic pacing mode for a campaign stage.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CampaignMode {
    Tps,
    Tpb,
}

/// Scenario weight for a stage.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampaignMixEntry {
    pub scenario: String,
    pub share_pct: f64,
}

/// A single spam stage within a campaign.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampaignStage {
    pub name: String,
    #[serde(default)]
    pub duration_secs: Option<u64>,
    #[serde(default)]
    pub duration_blocks: Option<u64>,
    #[serde(default)]
    pub rate: Option<u64>,
    #[serde(default)]
    pub tps: Option<u64>,
    #[serde(default)]
    pub tpb: Option<u64>,
    #[serde(default)]
    pub mix: Vec<CampaignMixEntry>,
}

/// Setup section – run once before spam stages.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct CampaignSetup {
    #[serde(default)]
    pub scenarios: Vec<String>,
}

/// Spam configuration shared across stages.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
pub struct CampaignSpam {
    #[serde(default)]
    pub mode: Option<CampaignMode>,
    #[serde(default)]
    pub rate: Option<u64>,
    #[serde(default)]
    pub tps: Option<u64>,
    #[serde(default)]
    pub tpb: Option<u64>,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub seed: Option<u64>,
    /// Maximum time in seconds for a stage to complete (separate from spam duration).
    /// If a stage exceeds this timeout, it will be terminated.
    #[serde(default)]
    pub stage_timeout: Option<u64>,
    #[serde(default)]
    pub stage: Vec<CampaignStage>,
    /// Shorthand for a single steady stage when no explicit `stage` entries are provided.
    #[serde(default)]
    pub mix: Option<Vec<CampaignMixEntry>>,
}

/// Composite / meta-scenario description.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampaignConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub setup: CampaignSetup,
    pub spam: CampaignSpam,
}

/// Resolved runtime parameters per stage after validation/defaulting.
#[derive(Clone, Debug)]
pub struct ResolvedStage {
    pub name: String,
    pub mode: CampaignMode,
    pub rate: u64,
    pub duration: u64,
    pub stage_timeout: Option<u64>,
    pub mix: Vec<ResolvedMixEntry>,
}

#[derive(Clone, Debug)]
pub struct ResolvedMixEntry {
    pub scenario: String,
    pub share_pct: f64,
    pub rate: u64,
}

fn validate_rate_fields(rate: Option<u64>, tps: Option<u64>, tpb: Option<u64>, context: &str) -> Result<()> {
    let set_count = rate.is_some() as usize + tps.is_some() as usize + tpb.is_some() as usize;
    if set_count > 1 {
        return Err(Error::Campaign(format!(
            "{context}: specify only one of rate/tps/tpb"
        )));
    }
    Ok(())
}

impl CampaignConfig {
    /// Parse a campaign from TOML file.
    pub fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        Self::from_toml_str(&contents)
    }

    /// Parse a campaign from raw TOML.
    pub fn from_toml_str(toml: &str) -> Result<Self> {
        let cfg: CampaignConfig = toml::from_str(toml)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Validate top-level and stage-level invariants.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::Campaign("campaign name must not be empty".into()));
        }
        // Normalize stages first so validation covers both explicit and shorthand forms.
        let normalized_stages = self.spam.normalized_stages()?;

        validate_rate_fields(
            self.spam.rate,
            self.spam.tps,
            self.spam.tpb,
            "campaign spam defaults",
        )?;

        for (idx, stage) in normalized_stages.iter().enumerate() {
            validate_rate_fields(
                stage.rate,
                stage.tps,
                stage.tpb,
                &format!("stage {} ({})", idx, stage.name),
            )?;
            if stage.mix.is_empty() {
                return Err(Error::Campaign(format!(
                    "stage {} ({}) must include at least one mix entry",
                    idx, stage.name
                )));
            }
            if stage.duration_secs.is_none()
                && stage.duration_blocks.is_none()
                && self.spam.duration.is_none()
            {
                return Err(Error::Campaign(format!(
                    "stage {} ({}) missing duration_secs/duration_blocks and no default duration provided",
                    idx, stage.name
                )));
            }
        }
        Ok(())
    }

    /// Normalize defaults and compute per-stage rates for execution.
    pub fn resolve(&self) -> Result<Vec<ResolvedStage>> {
        let normalized_stages = self.spam.normalized_stages()?;
        let default_mode = self
            .spam
            .mode
            .or_else(|| {
                if self.spam.tps.is_some() {
                    Some(CampaignMode::Tps)
                } else if self.spam.tpb.is_some() {
                    Some(CampaignMode::Tpb)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                Error::Campaign(
                    "campaign.spam.mode must be set when no tps/tpb default is provided (required when using rate)"
                        .into(),
                )
            })?;

        let mut resolved_stages = Vec::new();
        for stage in &normalized_stages {
            let mode = stage
                .tps
                .map(|_| CampaignMode::Tps)
                .or(stage.tpb.map(|_| CampaignMode::Tpb))
                .or(Some(default_mode))
                .expect("default mode checked");

            let rate = match mode {
                CampaignMode::Tps => stage
                    .tps
                    .or(stage.rate)
                    .or(self.spam.tps)
                    .or(self.spam.rate)
                    .ok_or_else(|| {
                        Error::Campaign(format!(
                            "stage {} requires rate (or spam.tps) when mode is tps",
                            stage.name
                        ))
                    })?,
                CampaignMode::Tpb => stage
                    .tpb
                    .or(stage.rate)
                    .or(self.spam.tpb)
                    .or(self.spam.rate)
                    .ok_or_else(|| {
                        Error::Campaign(format!(
                            "stage {} requires rate (or spam.tpb) when mode is tpb",
                            stage.name
                        ))
                    })?,
            };

            let duration = match mode {
                CampaignMode::Tps => stage.duration_secs.or(self.spam.duration).ok_or_else(|| {
                    Error::Campaign(format!(
                        "stage {} missing duration_secs and no default spam.duration provided",
                        stage.name
                    ))
                })?,
                CampaignMode::Tpb => stage
                    .duration_blocks
                    .or(self.spam.duration)
                    .ok_or_else(|| {
                        Error::Campaign(format!(
                            "stage {} missing duration_blocks and no default spam.duration provided",
                            stage.name
                        ))
                    })?,
            };

            let mix_sum: f64 = stage.mix.iter().map(|m| m.share_pct).sum();
            if mix_sum <= f64::EPSILON {
                return Err(Error::Campaign(format!(
                    "stage {} mix shares must sum to a positive number",
                    stage.name
                )));
            }

            // Normalize shares and compute integer rates; last entry absorbs rounding drift.
            // Note: This approach ensures the total rate matches exactly, but may result in
            // small discrepancies for individual scenarios due to rounding.
            let mut resolved_mix = Vec::new();
            let mut assigned = 0u64;
            for (idx, mix) in stage.mix.iter().enumerate() {
                let normalized_share = mix.share_pct / mix_sum;
                let mut scenario_rate = (rate as f64 * normalized_share).round() as u64;
                if idx == stage.mix.len() - 1 {
                    // Last entry gets exactly what's left to ensure total equals rate
                    let remaining = rate.saturating_sub(assigned);

                    // Validate that rounding didn't cause excessive drift
                    if assigned > rate {
                        return Err(Error::Campaign(format!(
                            "stage {}: rate distribution error - assigned {} exceeds total rate {}",
                            stage.name, assigned, rate
                        )));
                    }

                    // Warn if the adjustment is significant (more than 50% off from expected)
                    let expected = scenario_rate;
                    if expected > 0 && remaining > 0 {
                        let drift_pct = if remaining > expected {
                            ((remaining - expected) as f64 / expected as f64) * 100.0
                        } else {
                            ((expected - remaining) as f64 / expected as f64) * 100.0
                        };
                        // Only warn for significant drift (> 10%)
                        if drift_pct > 10.0 {
                            eprintln!(
                                "Warning: stage {} scenario {} rate adjusted from {} to {} ({:.1}% drift) due to rounding",
                                stage.name, mix.scenario, expected, remaining, drift_pct
                            );
                        }
                    }

                    scenario_rate = remaining;
                } else {
                    assigned = assigned.saturating_add(scenario_rate);
                }
                resolved_mix.push(ResolvedMixEntry {
                    scenario: mix.scenario.clone(),
                    share_pct: mix.share_pct,
                    rate: scenario_rate,
                });
            }

            resolved_stages.push(ResolvedStage {
                name: stage.name.clone(),
                mode,
                rate,
                duration,
                stage_timeout: self.spam.stage_timeout,
                mix: resolved_mix,
            });
        }

        Ok(resolved_stages)
    }
}

impl CampaignSpam {
    /// Normalize spam configuration into explicit stages, supporting shorthand `[spam] + [[spam.mix]]`.
    pub fn normalized_stages(&self) -> Result<Vec<CampaignStage>> {
        if !self.stage.is_empty() {
            if self.mix.is_some() {
                return Err(Error::Campaign(
                    "campaign spam: cannot define both spam.stage and spam.mix".into(),
                ));
            }
            return Ok(self.stage.clone());
        }

        if let Some(mix) = &self.mix {
            if mix.is_empty() {
                return Err(Error::Campaign(
                    "campaign spam: spam.mix must include at least one entry".into(),
                ));
            }
            let duration = self.duration.ok_or_else(|| {
                Error::Campaign("campaign spam: shorthand requires spam.duration".into())
            })?;

            let stage = CampaignStage {
                name: "steady".to_string(),
                duration_secs: Some(duration),
                duration_blocks: None,
                rate: self.rate,
                tps: self.tps,
                tpb: self.tpb,
                mix: mix.clone(),
            };
            return Ok(vec![stage]);
        }

        Err(Error::Campaign(
            "campaign spam: must define either spam.stage or spam.mix + spam.duration".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_prefers_stages_when_present() {
        let spam = CampaignSpam {
            mode: Some(CampaignMode::Tps),
            rate: Some(10),
            duration: Some(100),
            stage: vec![CampaignStage {
                name: "explicit".into(),
                duration_secs: Some(50),
                duration_blocks: None,
                rate: Some(10),
                tps: None,
                tpb: None,
                mix: vec![CampaignMixEntry {
                    scenario: "s1".into(),
                    share_pct: 100.0,
                }],
            }],
            ..Default::default()
        };

        let stages = spam.normalized_stages().unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].name, "explicit");
    }

    #[test]
    fn normalized_shorthand_builds_stage() {
        let spam = CampaignSpam {
            mode: Some(CampaignMode::Tps),
            rate: Some(20),
            duration: Some(600),
            mix: Some(vec![
                CampaignMixEntry {
                    scenario: "s1".into(),
                    share_pct: 60.0,
                },
                CampaignMixEntry {
                    scenario: "s2".into(),
                    share_pct: 40.0,
                },
            ]),
            ..Default::default()
        };

        let stages = spam.normalized_stages().unwrap();
        assert_eq!(stages.len(), 1);
        let s = &stages[0];
        assert_eq!(s.name, "steady");
        assert_eq!(s.duration_secs, Some(600));
        assert_eq!(s.rate, Some(20));
        assert_eq!(s.mix.len(), 2);
    }

    #[test]
    fn normalized_errors_when_both_stage_and_mix() {
        let spam = CampaignSpam {
            stage: vec![CampaignStage {
                name: "explicit".into(),
                duration_secs: Some(10),
                duration_blocks: None,
                rate: None,
                tps: Some(5),
                tpb: None,
                mix: vec![CampaignMixEntry {
                    scenario: "s1".into(),
                    share_pct: 100.0,
                }],
            }],
            mix: Some(vec![CampaignMixEntry {
                scenario: "s2".into(),
                share_pct: 100.0,
            }]),
            ..Default::default()
        };

        let err = spam.normalized_stages().unwrap_err();
        assert!(format!("{err}").contains("cannot define both"));
    }

    #[test]
    fn normalized_errors_when_missing_both() {
        let spam = CampaignSpam::default();
        let err = spam.normalized_stages().unwrap_err();
        assert!(format!("{err}").contains("must define either"));
    }

    #[test]
    fn resolve_shorthand_matches_explicit_single_stage() {
        let mix = vec![
            CampaignMixEntry {
                scenario: "s1".into(),
                share_pct: 60.0,
            },
            CampaignMixEntry {
                scenario: "s2".into(),
                share_pct: 40.0,
            },
        ];

        let explicit = CampaignConfig {
            name: "cmp".into(),
            description: None,
            setup: CampaignSetup {
                scenarios: vec!["s1".into(), "s2".into()],
            },
            spam: CampaignSpam {
                mode: Some(CampaignMode::Tps),
                rate: Some(20),
                duration: Some(600),
                stage: vec![CampaignStage {
                    name: "steady".into(),
                    duration_secs: Some(600),
                    duration_blocks: None,
                    rate: Some(20),
                    tps: None,
                    tpb: None,
                    mix: mix.clone(),
                }],
                ..Default::default()
            },
        };

        let shorthand = CampaignConfig {
            name: "cmp".into(),
            description: None,
            setup: CampaignSetup {
                scenarios: vec!["s1".into(), "s2".into()],
            },
            spam: CampaignSpam {
                mode: Some(CampaignMode::Tps),
                rate: Some(20),
                duration: Some(600),
                mix: Some(mix.clone()),
                ..Default::default()
            },
        };

        let explicit_resolved = explicit.resolve().unwrap();
        let shorthand_resolved = shorthand.resolve().unwrap();
        assert_eq!(explicit_resolved.len(), 1);
        assert_eq!(shorthand_resolved.len(), 1);
        let e = &explicit_resolved[0];
        let s = &shorthand_resolved[0];
        assert_eq!(e.name, s.name);
        assert_eq!(e.mode, s.mode);
        assert_eq!(e.rate, s.rate);
        assert_eq!(e.duration, s.duration);
        assert_eq!(e.mix.len(), s.mix.len());
        // scenario order and computed rates should match
        for (em, sm) in e.mix.iter().zip(s.mix.iter()) {
            assert_eq!(em.scenario, sm.scenario);
            assert_eq!(em.rate, sm.rate);
        }
    }

    #[test]
    fn validate_shorthand_passes() {
        let cfg = CampaignConfig {
            name: "cmp".into(),
            description: None,
            setup: CampaignSetup {
                scenarios: vec!["s1".into()],
            },
            spam: CampaignSpam {
                mode: Some(CampaignMode::Tps),
                rate: Some(5),
                duration: Some(30),
                mix: Some(vec![CampaignMixEntry {
                    scenario: "s1".into(),
                    share_pct: 100.0,
                }]),
                ..Default::default()
            },
        };

        cfg.validate().unwrap();
    }
}
