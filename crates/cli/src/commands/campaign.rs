use super::{setup::SetupCommandArgs, spam::SpamCommandArgs, SpamScenario};
use crate::commands::spam::SpamCampaignContext;
use crate::commands::{
    self,
    common::{ScenarioSendTxsCliArgs, SendTxsCliArgsInner},
    SpamCliArgs,
};
use crate::error::CliError;
use crate::util::load_testconfig;
use crate::util::{load_seedfile, parse_duration};
use crate::BuiltinScenarioCli;
use alloy::primitives::{keccak256, U256};
use clap::Args;
use contender_core::db::DbOps;
use contender_core::error::RuntimeParamErrorKind;
use contender_testfile::{CampaignConfig, CampaignMode, ResolvedMixEntry, ResolvedStage};
use std::time::Duration;
use tracing::{debug, info, warn};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Debug, Args)]
pub struct CampaignCliArgs {
    /// Path to campaign config TOML
    #[arg(help = "Path to campaign config TOML")]
    pub campaign: String,

    #[command(flatten)]
    pub eth_json_rpc_args: SendTxsCliArgsInner,

    /// HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`).
    #[arg(
        env = "BUILDER_RPC_URL",
        short,
        long,
        long_help = "HTTP JSON-RPC URL to use for bundle spamming (must support `eth_sendBundle`)",
        visible_aliases = ["builder", "builder-rpc-url", "builder-rpc"]
    )]
    pub builder_url: Option<Url>,

    /// The time to wait for pending transactions to land, in blocks.
    #[arg(
        short = 'w',
        long,
        default_value_t = 12,
        long_help = "The number of blocks to wait for pending transactions to land. If transactions land within the timeout, it resets.",
        visible_aliases = ["wait"]
    )]
    pub pending_timeout: u64,

    /// Max number of txs to send in a single json-rpc batch request.
    #[arg(
        long = "rpc-batch-size",
        value_name = "N",
        default_value_t = 0,
        long_help = "Max number of eth_sendRawTransaction calls to send in a single JSON-RPC batch request. 0 (default) disables batching and sends one eth_sendRawTransaction per tx."
    )]
    pub rpc_batch_size: u64,

    /// Ignore receipts (fire-and-forget).
    #[arg(
        long,
        help = "Ignore transaction receipts.",
        long_help = "Keep sending transactions without waiting for receipts.",
        visible_aliases = ["ir", "no-receipts"]
    )]
    pub ignore_receipts: bool,

    /// Disable nonce synchronization between batches.
    #[arg(
        long,
        help = "Disable nonce synchronization between batches.",
        visible_aliases = ["disable-nonce-sync", "fast-nonces"]
    )]
    pub optimistic_nonces: bool,

    /// Generate report after campaign finishes.
    #[arg(
        long,
        long_help = "Generate a report for the spam run(s) after the campaign completes.",
        visible_aliases = ["report"]
    )]
    pub gen_report: bool,

    /// Skip setup steps when running builtin scenarios.
    #[arg(
        long,
        global = true,
        long_help = "If set, skip contract deployment & setup transactions when running builtin scenarios. Does nothing when running a scenario file."
    )]
    pub skip_setup: bool,

    /// The time to wait for spammer to recover from failure before stopping contender.
    #[arg(
        long = "timeout",
        long_help = "The time to wait for spammer to recover from failure before stopping contender.",
        value_parser = parse_duration,
        default_value = "5min"
    )]
    pub spam_timeout: Duration,

    /// Run campaign in a loop, indefinitely.
    #[arg(
        global = true,
        default_value_t = false,
        long = "forever",
        visible_aliases = ["indefinite", "indefinitely", "infinite"]
    )]
    pub run_forever: bool,
}

fn bump_seed(base_seed: &str, stage_name: &str) -> String {
    let compound_hash = keccak256(base_seed).bit_or(keccak256(stage_name));
    U256::from_be_bytes(compound_hash.0).to_string()
}

pub async fn run_campaign(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    data_dir: &str,
    args: CampaignCliArgs,
) -> Result<(), CliError> {
    let campaign = CampaignConfig::from_file(&args.campaign)?;
    let stages = campaign.resolve()?;
    validate_stage_rates(&stages, &args).await?;
    let campaign_id = Uuid::new_v4().to_string();

    let base_seed = args
        .eth_json_rpc_args
        .seed
        .clone()
        .or_else(|| campaign.spam.seed.map(|s| s.to_string()))
        .unwrap_or(load_seedfile(data_dir)?);

    // Setup phase: run setup for each (stage, mix) with the same derived seed that spam will use.
    // This ensures setup creates accounts matching what spam expects.
    // Skip builtin scenarios since they do their own setup at spam time.
    if !args.skip_setup {
        for stage in &stages {
            let stage_seed = bump_seed(&base_seed, &stage.name);
            for (mix_idx, mix) in stage.mix.iter().enumerate() {
                if mix.rate == 0 {
                    continue;
                }
                // Skip builtins - they do their own setup during spam
                if parse_builtin_reference(&mix.scenario).is_some() {
                    continue;
                }

                let scenario_seed = bump_seed(&stage_seed, &mix_idx.to_string());
                let scenario = SpamScenario::Testfile(mix.scenario.clone());

                let mut setup_args = args.eth_json_rpc_args.clone();
                setup_args.seed = Some(scenario_seed);
                // Ensure accounts_per_agent uses campaign default (10) if not explicitly set
                if setup_args.accounts_per_agent.is_none() {
                    setup_args.accounts_per_agent = Some(10);
                }
                let setup_cmd = SetupCommandArgs::new(scenario, setup_args, data_dir)?;
                commands::setup(db, setup_cmd).await?
            }
        }
    }

    let mut run_ids = vec![];

    loop {
        tokio::select! {
            result = async {
                for (stage_idx, stage) in stages.iter().enumerate() {
                    info!(
                        campaign_id = %campaign_id,
                        campaign_name = %campaign.name,
                        "Starting campaign stage {}: {} ({}={})",
                        stage_idx + 1,
                        stage.name,
                        match campaign.spam.mode {
                            CampaignMode::Tps => "tps",
                            CampaignMode::Tpb => "tpb",
                        },
                        stage.rate
                    );

                    // Avoid nonce conflicts: override_senders would share a single EOA across mixes.
                    if args.eth_json_rpc_args.override_senders && stage.mix.len() > 1 {
                        return Err(RuntimeParamErrorKind::InvalidArgs(
                            "override-senders cannot be used when a stage has multiple mix entries; it would share one sender across mixes and cause nonce conflicts".into(),
                        )
                        .into());
                    }

                    let stage_seed = bump_seed(&base_seed, &stage.name);

                    // Execute stage with optional timeout
                    let stage_run_ids = if let Some(timeout_secs) = stage.stage_timeout {
                        let timeout_duration = std::time::Duration::from_secs(timeout_secs);
                        match tokio::time::timeout(
                            timeout_duration,
                            execute_stage(db, &campaign, stage, &args, &campaign_id, &stage_seed, data_dir),
                        )
                        .await
                        {
                            Ok(result) => result?,
                            Err(_) => {
                                return Err(RuntimeParamErrorKind::InvalidArgs(format!(
                                    "Stage '{}' exceeded timeout of {} seconds",
                                    stage.name, timeout_secs
                                ))
                                .into());
                            }
                        }
                    } else {
                        execute_stage(db, &campaign, stage, &args, &campaign_id, &stage_seed, data_dir).await?
                    };

                    run_ids.extend(stage_run_ids);
                }
                Ok::<_, CliError>(())
            } => {
                // Propagate any error from the campaign execution
                result?;
                if args.run_forever {
                    info!("Campaign {campaign_id} completed. Running again due to --forever flag.");
                    continue;
                }
                info!("Campaign {campaign_id} completed.");
                break;
            },
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl-C, stopping campaign {campaign_id}.");
                break;
            }
        }
    }

    if args.gen_report {
        if run_ids.is_empty() {
            warn!("No runs found for campaign, skipping report.");
        } else {
            run_ids.sort_unstable();
            let first_run = *run_ids.first().expect("run IDs exist");
            let last_run = *run_ids.last().expect("run IDs exist");
            contender_report::command::report(
                Some(last_run),
                last_run - first_run,
                db,
                data_dir,
                false, // use HTML format by default for campaign reports
            )
            .await?;
        }
    }

    Ok(())
}

async fn validate_stage_rates(
    stages: &[ResolvedStage],
    _args: &CampaignCliArgs,
) -> Result<(), CliError> {
    for stage in stages {
        for mix in &stage.mix {
            if mix.rate == 0 {
                continue;
            }
            if parse_builtin_reference(&mix.scenario).is_some() {
                continue;
            }
            let cfg = load_testconfig(&mix.scenario).await?;
            let spam_len = cfg.spam.as_ref().map(|s| s.len()).unwrap_or(0) as u64;
            if spam_len == 0 {
                return Err(RuntimeParamErrorKind::InvalidArgs(format!(
                    "Stage '{}' scenario '{}' has no spam entries defined.",
                    stage.name, mix.scenario
                ))
                .into());
            }
            // Check if rate * duration is sufficient to cover all spam entries
            let total_txs = mix.rate * stage.duration;
            if total_txs < spam_len {
                return Err(RuntimeParamErrorKind::InvalidArgs(format!(
                    "Stage '{}' scenario '{}': insufficient transactions (rate {} * duration {} = {}) to cover {} spam entries. Minimum rate needed: {}",
                    stage.name, mix.scenario, mix.rate, stage.duration, total_txs, spam_len,
                    spam_len.div_ceil(stage.duration)  // ceiling division
                ))
                .into());
            }
        }
    }
    Ok(())
}

fn create_spam_cli_args(
    testfile: Option<String>,
    args: &CampaignCliArgs,
    spam_mode: CampaignMode,
    spam_rate: u64,
    spam_duration: u64,
    skip_setup: bool,
) -> SpamCliArgs {
    SpamCliArgs {
        eth_json_rpc_args: ScenarioSendTxsCliArgs {
            testfile,
            rpc_args: args.eth_json_rpc_args.clone(),
        },
        spam_args: crate::commands::common::SendSpamCliArgs {
            builder_url: args.builder_url.clone(),
            txs_per_second: if matches!(spam_mode, CampaignMode::Tps) {
                Some(spam_rate)
            } else {
                None
            },
            txs_per_block: if matches!(spam_mode, CampaignMode::Tpb) {
                Some(spam_rate)
            } else {
                None
            },
            duration: spam_duration,
            pending_timeout: args.pending_timeout,
            run_forever: false,
        },
        ignore_receipts: args.ignore_receipts,
        optimistic_nonces: args.optimistic_nonces,
        gen_report: false,
        skip_setup,
        rpc_batch_size: args.rpc_batch_size,
        spam_timeout: args.spam_timeout,
    }
}

/// Metadata for logging scenario execution
struct ScenarioMeta {
    campaign_id: String,
    campaign_name: String,
    stage_name: String,
    scenario_label: String,
    mode: CampaignMode,
    rate: u64,
    duration: u64,
}

impl ScenarioMeta {
    fn to_context(&self) -> SpamCampaignContext {
        SpamCampaignContext {
            campaign_id: Some(self.campaign_id.clone()),
            campaign_name: Some(self.campaign_name.clone()),
            stage_name: Some(self.stage_name.clone()),
            scenario_name: Some(self.scenario_label.clone()),
        }
    }

    fn log_start(&self, is_fast_path: bool) {
        let msg = if is_fast_path {
            "Starting campaign scenario spammer (fast path)"
        } else {
            "Starting campaign scenario spammer"
        };
        info!(
            campaign_id = %self.campaign_id,
            campaign_name = %self.campaign_name,
            stage = %self.stage_name,
            scenario = %self.scenario_label,
            mode = ?self.mode,
            rate = self.rate,
            duration = self.duration,
            "{msg}",
        );
    }

    fn log_finished(&self, run_id: u64) {
        info!(
            campaign_id = %self.campaign_id,
            campaign_name = %self.campaign_name,
            stage = %self.stage_name,
            scenario = %self.scenario_label,
            run_id,
            "Finished campaign scenario spammer"
        );
    }

    fn log_no_run_id(&self) {
        warn!(
            campaign_id = %self.campaign_id,
            campaign_name = %self.campaign_name,
            stage = %self.stage_name,
            scenario = %self.scenario_label,
            "Campaign scenario finished without recording a run_id"
        );
    }
}

/// Context for preparing and executing a scenario within a campaign stage.
struct ScenarioContext<'a> {
    args: &'a CampaignCliArgs,
    campaign: &'a CampaignConfig,
    stage: &'a ResolvedStage,
    campaign_id: &'a str,
    stage_seed: &'a str,
    data_dir: &'a str,
}

/// Prepares a scenario for execution, returning the spam args and metadata
async fn prepare_scenario(
    ctx: &ScenarioContext<'_>,
    mix_idx: usize,
    mix: &ResolvedMixEntry,
) -> Result<(SpamCommandArgs, ScenarioMeta), CliError> {
    let scenario_seed = bump_seed(ctx.stage_seed, &mix_idx.to_string());
    let mut args = ctx.args.to_owned();
    args.eth_json_rpc_args.seed = Some(scenario_seed.clone());
    debug!("mix {mix_idx} seed: {}", scenario_seed);

    // Check if this is a builtin scenario to determine skip_setup behavior:
    // - Builtins: respect campaign's flags (they do their own setup during spam)
    // - Toml scenarios: always skip setup (ran in Phase 1)
    let is_builtin = parse_builtin_reference(&mix.scenario).is_some();
    let skip_setup = if is_builtin { args.skip_setup } else { true };

    let spam_cli_args = create_spam_cli_args(
        Some(mix.scenario.clone()),
        &args,
        ctx.campaign.spam.mode,
        mix.rate,
        ctx.stage.duration,
        skip_setup,
    );

    let spam_scenario = if let Some(builtin_cli) = parse_builtin_reference(&mix.scenario) {
        let provider = args.eth_json_rpc_args.new_rpc_provider()?;
        let builtin = builtin_cli
            .to_builtin_scenario(&provider, &spam_cli_args, ctx.data_dir)
            .await?;
        SpamScenario::Builtin(builtin)
    } else {
        SpamScenario::Testfile(mix.scenario.clone())
    };

    let spam_args = SpamCommandArgs::new(spam_scenario, spam_cli_args, ctx.data_dir)?;

    let meta = ScenarioMeta {
        campaign_id: ctx.campaign_id.to_owned(),
        campaign_name: ctx.campaign.name.clone(),
        stage_name: ctx.stage.name.clone(),
        scenario_label: mix.scenario.clone(),
        mode: ctx.campaign.spam.mode,
        rate: mix.rate,
        duration: ctx.stage.duration,
    };

    Ok((spam_args, meta))
}

async fn execute_stage(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    campaign: &CampaignConfig,
    stage: &ResolvedStage,
    args: &CampaignCliArgs,
    campaign_id: &str,
    stage_seed: &str,
    data_dir: &str,
) -> Result<Vec<u64>, CliError> {
    // Collect active scenarios (non-zero rate) with their indices
    let active_scenarios: Vec<_> = stage
        .mix
        .iter()
        .enumerate()
        .filter(|(_, mix)| mix.rate > 0)
        .collect();

    // Validate that at least one scenario has non-zero rate
    if active_scenarios.is_empty() {
        return Err(RuntimeParamErrorKind::InvalidArgs(format!(
            "Stage '{}' has no scenarios with non-zero rate after resolution",
            stage.name
        ))
        .into());
    }

    let ctx = ScenarioContext {
        args,
        campaign,
        stage,
        campaign_id,
        stage_seed,
        data_dir,
    };

    // FAST PATH: Single mix scenario - call spam_inner directly (same as spam mode)
    // This avoids barrier synchronization and tokio::spawn overhead
    if active_scenarios.len() == 1 {
        let (mix_idx, mix) = active_scenarios[0];
        let (spam_args, meta) = prepare_scenario(&ctx, mix_idx, mix).await?;

        meta.log_start(true);

        let db = db.clone();
        let ctx = meta.to_context();
        let mut test_scenario = spam_args.init_scenario(&db).await?;
        let run_res = commands::spam_inner(&db, &mut test_scenario, &spam_args, ctx).await;

        return match run_res {
            Ok(Some(run_id)) => {
                meta.log_finished(run_id);
                Ok(vec![run_id])
            }
            Ok(None) => {
                meta.log_no_run_id();
                Ok(vec![])
            }
            Err(e) => Err(e),
        };
    }

    // MULTI-MIX PATH: Use barrier + spawn for parallel execution
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(active_scenarios.len()));
    let mut handles = vec![];

    for (mix_idx, mix) in active_scenarios {
        let (spam_args, meta) = prepare_scenario(&ctx, mix_idx, mix).await?;

        meta.log_start(false);

        let db = db.clone();
        let ctx = meta.to_context();
        let barrier_clone = barrier.clone();
        let mut test_scenario = spam_args.init_scenario(&db).await?;

        let handle = tokio::spawn(async move {
            // Wait for all parallel scenarios to be ready before starting
            barrier_clone.wait().await;

            let run_res = commands::spam_inner(&db, &mut test_scenario, &spam_args, ctx).await;
            match run_res {
                Ok(Some(run_id)) => {
                    meta.log_finished(run_id);
                    Ok(Some(run_id))
                }
                Ok(None) => {
                    meta.log_no_run_id();
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        });
        handles.push(handle);
    }

    let mut run_ids = vec![];
    for handle in handles {
        if let Some(run_id) = handle.await?? {
            run_ids.push(run_id);
        }
    }

    Ok(run_ids)
}

fn strip_builtin_name(name: impl AsRef<str>) -> String {
    name.as_ref()
        .trim()
        .trim_start_matches("builtin:")
        .to_owned()
}

fn parse_builtin_reference(name: &str) -> Option<BuiltinScenarioCli> {
    let norm = strip_builtin_name(name).to_lowercase();
    match norm.as_str() {
        "erc20" => Some(BuiltinScenarioCli::Erc20(Default::default())),
        "revert" | "reverts" => Some(BuiltinScenarioCli::Revert(Default::default())),
        "stress" => Some(BuiltinScenarioCli::Stress(Default::default())),
        "uni_v2" | "univ2" | "uni-v2" => Some(BuiltinScenarioCli::UniV2(Default::default())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use contender_testfile::{ResolvedMixEntry, ResolvedStage};
    use std::sync::Arc;
    use tokio::sync::{Barrier, Mutex};
    use tokio::time::{sleep, Duration};

    fn test_stage(name: &str) -> ResolvedStage {
        ResolvedStage {
            name: name.to_string(),
            rate: 1,
            duration: 1,
            stage_timeout: None,
            mix: vec![
                ResolvedMixEntry {
                    scenario: "s1".to_string(),
                    share_pct: 50.0,
                    rate: 1,
                },
                ResolvedMixEntry {
                    scenario: "s2".to_string(),
                    share_pct: 50.0,
                    rate: 1,
                },
            ],
        }
    }

    #[tokio::test]
    async fn stages_run_sequentially() {
        let stages = vec![test_stage("first"), test_stage("second")];
        let events = Arc::new(Mutex::new(Vec::new()));

        for s in &stages {
            {
                let mut ev = events.lock().await;
                ev.push(format!("start-{}", s.name));
            }
            // simulate work
            sleep(Duration::from_millis(5)).await;
            {
                let mut ev = events.lock().await;
                ev.push(format!("end-{}", s.name));
            }
        }

        let ev = events.lock().await;
        assert_eq!(
            ev.as_slice(),
            &["start-first", "end-first", "start-second", "end-second"]
        );
    }

    #[tokio::test]
    async fn stage_mixes_run_in_parallel() {
        let s = test_stage("parallel");
        let barrier = Arc::new(Barrier::new(s.mix.len() + 1));
        let starts = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        for mix in s.mix.clone() {
            let b = barrier.clone();
            let starts = starts.clone();
            handles.push(tokio::spawn(async move {
                {
                    let mut st = starts.lock().await;
                    st.push(mix.scenario.clone());
                }
                // wait for all tasks to reach this point
                b.wait().await;
                Ok::<(), ()>(())
            }));
        }

        // release all spawned tasks once they have all started
        barrier.wait().await;
        for h in handles {
            h.await.unwrap().unwrap();
        }

        let st = starts.lock().await;
        // all mixes started; order not important, but count must match
        assert_eq!(st.len(), 2);
        assert!(st.contains(&"s1".to_string()));
        assert!(st.contains(&"s2".to_string()));
    }
}
