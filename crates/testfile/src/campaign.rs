use crate::{error::CampaignError, Result};
use serde::{Deserialize, Serialize};

/// Defines the traffic pacing mode for a campaign stage.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CampaignMode {
    Tps,
    Tpb,
}

impl Default for CampaignMode {
    fn default() -> Self {
        Self::Tps
    }
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
    pub duration: Option<u64>,
    pub rate: Option<u64>,
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
    pub mode: CampaignMode,
    pub rate: Option<u64>,
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
            return Err(CampaignError::NameEmpty.into());
        }
        // Normalize stages first so validation covers both explicit and shorthand forms.
        let normalized_stages = self.spam.normalized_stages()?;

        for (idx, stage) in normalized_stages.iter().enumerate() {
            if stage.mix.is_empty() {
                return Err(CampaignError::StageMixEmpty {
                    index: idx,
                    name: stage.name.clone(),
                }
                .into());
            }
            if stage.duration.is_none() && self.spam.duration.is_none() {
                return Err(CampaignError::SpamDurationMissing {
                    index: idx,
                    name: stage.name.clone(),
                }
                .into());
            }
        }
        Ok(())
    }

    /// Normalize defaults and compute per-stage rates for execution.
    pub fn resolve(&self) -> Result<Vec<ResolvedStage>> {
        let normalized_stages = self.spam.normalized_stages()?;

        let mut resolved_stages = Vec::new();
        for (idx, stage) in normalized_stages.iter().enumerate() {
            let duration = stage.duration.unwrap_or(self.spam.duration.ok_or(
                CampaignError::SpamDurationMissing {
                    index: idx,
                    name: stage.name.clone(),
                },
            )?);

            let mix_sum: f64 = stage.mix.iter().map(|m| m.share_pct).sum();
            if mix_sum <= f64::EPSILON {
                return Err(CampaignError::MixSharesSumInvalid {
                    name: stage.name.clone(),
                }
                .into());
            }
            let rate = stage
                .rate
                .unwrap_or(self.spam.rate.ok_or(CampaignError::SpamRateMissing)?);

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
                        return Err(CampaignError::RateDistributionExceedsLimit {
                            name: stage.name.clone(),
                            assigned_rate: assigned,
                            total_rate: rate,
                        }
                        .into());
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
                return Err(CampaignError::ConflictingMixAndStage.into());
            }
            return Ok(self.stage.clone());
        }

        if let Some(mix) = &self.mix {
            if mix.is_empty() {
                return Err(CampaignError::SpamMixEmpty.into());
            }
            let duration = self
                .duration
                .ok_or(CampaignError::ShorthandRequiresSpamDuration)?;

            let stage = CampaignStage {
                name: "steady".to_string(),
                duration: Some(duration),
                rate: self.rate,
                mix: mix.clone(),
            };
            return Ok(vec![stage]);
        }

        Err(CampaignError::SpamStageOrMixUndefined.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_prefers_stages_when_present() {
        let spam = CampaignSpam {
            rate: Some(10),
            duration: Some(100),
            stage: vec![CampaignStage {
                name: "explicit".into(),
                duration: Some(50),
                rate: Some(10),
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
            mode: CampaignMode::Tps,
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
        assert_eq!(s.duration, Some(600));
        assert_eq!(s.rate, Some(20));
        assert_eq!(s.mix.len(), 2);
    }

    #[test]
    fn normalized_errors_when_both_stage_and_mix() {
        let spam = CampaignSpam {
            stage: vec![CampaignStage {
                name: "explicit".into(),
                duration: Some(10),
                rate: Some(5),
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
                mode: CampaignMode::Tps,
                rate: Some(20),
                duration: Some(600),
                stage: vec![CampaignStage {
                    name: "steady".into(),
                    duration: Some(600),
                    rate: Some(20),
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
                mode: CampaignMode::Tps,
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
                mode: CampaignMode::Tps,
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
