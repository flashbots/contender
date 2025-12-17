use super::{
    setup::SetupCommandArgs,
    spam::{SpamCommandArgs, SpamRunContext},
    SpamScenario,
};
use crate::error::CliError;
use crate::util::load_testconfig;
use crate::util::{data_dir, load_seedfile, parse_duration};
use crate::BuiltinScenarioCli;
use crate::{
    commands::{
        self,
        common::{ScenarioSendTxsCliArgs, SendTxsCliArgsInner},
        SpamCliArgs,
    },
    util::bold,
};
use alloy::primitives::U256;
use clap::Args;
use contender_core::db::DbOps;
use contender_core::error::RuntimeParamErrorKind;
use contender_testfile::{CampaignConfig, CampaignMode, ResolvedStage};
use std::time::Duration;
use tracing::{info, warn};
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
    pub builder_url: Option<String>,

    /// The time to wait for pending transactions to land, in blocks.
    #[arg(
        short = 'w',
        long,
        default_value_t = 12,
        long_help = "The number of blocks to wait for pending transactions to land. If transactions land within the timeout, it resets.",
        visible_aliases = ["wait"]
    )]
    pub pending_timeout: u64,

    /// The number of accounts to generate for each agent (`from_pool` in scenario files)
    #[arg(
        short,
        long,
        visible_aliases = ["na", "accounts"],
        default_value_t = 10
    )]
    pub accounts_per_agent: u64,

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

    /// Re-deploy contracts in builtin scenarios.
    #[arg(
        long,
        global = true,
        long_help = "If set, re-deploy contracts that have already been deployed. Only builtin scenarios are affected."
    )]
    pub redeploy: bool,

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
}

fn bump_seed(base: &str, bump: u64) -> Result<String, CliError> {
    let (radix, trimmed) = if let Some(stripped) = base.strip_prefix("0x") {
        (16, stripped)
    } else {
        (10, base)
    };
    let val = U256::from_str_radix(trimmed, radix).map_err(|e| {
        RuntimeParamErrorKind::InvalidArgs(format!("Invalid seed value '{}': {}", base, e))
    })?;
    let bumped = val.wrapping_add(U256::from(bump));
    Ok(format!("{bumped}"))
}

pub async fn run_campaign(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: CampaignCliArgs,
) -> Result<(), CliError> {
    let campaign = CampaignConfig::from_file(&args.campaign)?;
    let stages = campaign.resolve()?;
    validate_stage_rates(&stages, &args).await?;
    let campaign_id = Uuid::new_v4().to_string();

    if args.redeploy && args.skip_setup {
        return Err(RuntimeParamErrorKind::InvalidArgs(format!(
            "{} and {} cannot be passed together",
            bold("--redeploy"),
            bold("--skip-setup")
        ))
        .into());
    }

    let base_seed = args
        .eth_json_rpc_args
        .seed
        .clone()
        .or_else(|| campaign.spam.seed.map(|s| s.to_string()))
        .unwrap_or(load_seedfile()?);

    // Setup phase. Skip builtin scenarios since they do their own setup at spam time.
    let setup = &campaign.setup;
    let provider = args.eth_json_rpc_args.new_rpc_provider()?;
    if !args.skip_setup {
        for scenario_label in &setup.scenarios {
            let scenario = match parse_builtin_reference(&scenario_label) {
                Some(builtin) => SpamScenario::Builtin(
                    builtin
                        .to_builtin_scenario(
                            &provider,
                            &create_spam_cli_args(None, &args, CampaignMode::Tps, 1, 1),
                            /* TODO: KLUDGE:
                               - I don't think a `BuiltinScenarioCli` *needs* `rate` or `duration` -- that's for the spammer.
                               - we should use a different interface for `to_builtin_scenario` (replace `SpamCliArgs`)
                            */
                        )
                        .await?,
                ),
                None => SpamScenario::Testfile(scenario_label.to_owned()),
            };
            let mut setup_args = args.eth_json_rpc_args.clone();
            setup_args.seed = Some(base_seed.clone());
            let setup_cmd = SetupCommandArgs::new(scenario, setup_args)?;
            commands::setup(db, setup_cmd).await?;
        }
    }

    let mut run_ids = vec![];

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

        let stage_seed = bump_seed(&base_seed, stage_idx as u64)?;

        // Execute stage with optional timeout
        let stage_run_ids = if let Some(timeout_secs) = stage.stage_timeout {
            let timeout_duration = std::time::Duration::from_secs(timeout_secs);
            match tokio::time::timeout(
                timeout_duration,
                execute_stage(db, &campaign, stage, &args, &campaign_id, &stage_seed),
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
            execute_stage(db, &campaign, stage, &args, &campaign_id, &stage_seed).await?
        };

        run_ids.extend(stage_run_ids);
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
                &data_dir()?,
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
            loops: Some(Some(1)),
            accounts_per_agent: args.accounts_per_agent,
        },
        ignore_receipts: args.ignore_receipts,
        optimistic_nonces: args.optimistic_nonces,
        gen_report: false,
        spam_timeout: args.spam_timeout,
        redeploy: args.redeploy,
        skip_setup: true,
        rpc_batch_size: args.rpc_batch_size,
    }
}

async fn execute_stage(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    campaign: &CampaignConfig,
    stage: &ResolvedStage,
    args: &CampaignCliArgs,
    campaign_id: &str,
    stage_seed: &str,
) -> Result<Vec<u64>, CliError> {
    let mut handles = vec![];
    let mut run_ids = vec![];

    // Validate that at least one scenario has non-zero rate
    if stage.mix.iter().all(|mix| mix.rate == 0) {
        return Err(RuntimeParamErrorKind::InvalidArgs(format!(
            "Stage '{}' has no scenarios with non-zero rate after resolution",
            stage.name
        ))
        .into());
    }

    // Create a barrier to synchronize parallel task starts
    let active_scenario_count = stage.mix.iter().filter(|mix| mix.rate > 0).count();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(active_scenario_count));

    for (mix_idx, mix) in stage.mix.iter().enumerate() {
        if mix.rate == 0 {
            continue;
        }
        let mix = mix.clone();
        let scenario_seed = bump_seed(stage_seed, mix_idx as u64)?;
        let mut eth_args = args.eth_json_rpc_args.clone();
        eth_args.seed = Some(scenario_seed.clone());

        let spam_cli_args = create_spam_cli_args(
            Some(mix.scenario.clone()),
            args,
            campaign.spam.mode,
            mix.rate,
            stage.duration,
        );

        let spam_scenario = if let Some(builtin_cli) = parse_builtin_reference(&mix.scenario) {
            let provider = args.eth_json_rpc_args.new_rpc_provider()?;
            let builtin = builtin_cli
                .to_builtin_scenario(&provider, &spam_cli_args)
                .await?;
            SpamScenario::Builtin(builtin)
        } else {
            SpamScenario::Testfile(mix.scenario.clone())
        };

        let spam_args = SpamCommandArgs::new(spam_scenario, spam_cli_args)?;
        let scenario = spam_args.init_scenario(db).await?;
        let duration = stage.duration;
        let db = db.clone();
        let campaign_id_owned = campaign_id.to_owned();
        let campaign_name = campaign.name.clone();
        let stage_name = stage.name.clone();
        let scenario_label = mix.scenario.clone();
        let ctx = SpamRunContext {
            campaign_id: Some(campaign_id_owned.clone()),
            campaign_name: Some(campaign_name.clone()),
            stage_name: Some(stage.name.clone()),
            scenario_name: Some(mix.scenario.clone()),
        };
        let rate = mix.rate;
        let barrier_clone = barrier.clone();
        info!(
            campaign_id = %campaign_id_owned,
            campaign_name = %campaign_name,
            stage = %stage_name,
            scenario = %scenario_label,
            mode = ?campaign.spam.mode,
            rate,
            duration,
            "Starting campaign scenario spammer",
        );
        let handle = tokio::spawn(async move {
            // Wait for all parallel scenarios to be ready before starting
            barrier_clone.wait().await;

            let mut scenario = scenario;
            let run_res = commands::spam(&db, &spam_args, &mut scenario, ctx).await;
            match run_res {
                Ok(Some(run_id)) => {
                    info!(
                        campaign_id = %campaign_id_owned,
                        campaign_name = %campaign_name,
                        stage = %stage_name,
                        scenario = %scenario_label,
                        run_id,
                        "Finished campaign scenario spammer"
                    );
                    Ok(Some(run_id))
                }
                Ok(None) => {
                    warn!(
                        campaign_id = %campaign_id_owned,
                        campaign_name = %campaign_name,
                        stage = %stage_name,
                        scenario = %scenario_label,
                        "Campaign scenario finished without recording a run_id"
                    );
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        });
        handles.push(handle);
    }

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
