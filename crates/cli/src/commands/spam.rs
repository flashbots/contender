use super::common::{ScenarioSendTxsCliArgs, SendSpamCliArgs};
use crate::{
    commands::{
        common::{EngineParams, SendTxsCliArgsInner, TxTypeCli},
        error::ArgsError,
        Result,
    },
    default_scenarios::BuiltinScenario,
    error::CliError,
    util::{
        bold, check_private_keys, fund_accounts, load_seedfile, load_testconfig, parse_duration,
        provider::AuthClient,
    },
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    primitives::{utils::format_ether, U256},
    providers::Provider,
    transports::http::reqwest::Url,
};
use contender_core::{
    db::{DbOps, SpamDuration, SpamRunRequest},
    error::{RuntimeErrorKind, RuntimeParamErrorKind},
    generator::{
        agent_pools::{AgentPools, AgentSpec},
        seeder::rand_seed::SeedGenerator,
        templater::Templater,
        types::{AnyProvider, SpamRequest},
        PlanConfig, RandSeed,
    },
    spammer::{
        tx_actor::ActorContext, BlockwiseSpammer, LogCallback, NilCallback, Spammer, TimedSpammer,
    },
    test_scenario::{TestScenario, TestScenarioParams},
    util::get_block_time,
};
use contender_engine_provider::{
    reth_node_api::EngineApiMessageVersion, AuthProvider, ControlChain,
};
use contender_testfile::TestConfig;
use op_alloy_network::{Ethereum, Optimism};
use std::{path::PathBuf, sync::atomic::AtomicBool};
use std::{sync::Arc, time::Duration};
use tracing::{info, warn};

#[derive(Debug)]
pub struct EngineArgs {
    pub auth_rpc_url: Url,
    pub jwt_secret: PathBuf,
    pub use_op: bool,
    pub message_version: EngineApiMessageVersion,
}

impl EngineArgs {
    pub async fn new_provider(&self) -> Result<AuthClient> {
        let provider: Box<dyn ControlChain + Send + Sync + 'static> = if self.use_op {
            Box::new(
                AuthProvider::<Optimism>::from_jwt_file(
                    &self.auth_rpc_url,
                    &self.jwt_secret,
                    self.message_version,
                )
                .await?,
            )
        } else {
            Box::new(
                AuthProvider::<Ethereum>::from_jwt_file(
                    &self.auth_rpc_url,
                    &self.jwt_secret,
                    self.message_version,
                )
                .await?,
            )
        };
        Ok(AuthClient::new(provider))
    }
}

#[derive(Clone, Debug, clap::Args)]
pub struct SpamCliArgs {
    #[command(flatten)]
    pub eth_json_rpc_args: ScenarioSendTxsCliArgs,

    #[command(flatten)]
    pub spam_args: SendSpamCliArgs,

    #[arg(
        long,
        help = "Ignore transaction receipts.",
        long_help = "Keep sending transactions without waiting for receipts.",
        visible_aliases = ["ir", "no-receipts"]
    )]
    pub ignore_receipts: bool,

    #[arg(
        long,
        help = "Disable nonce synchronization between batches.",
        visible_aliases = ["disable-nonce-sync", "fast-nonces"]
    )]
    pub optimistic_nonces: bool,

    #[arg(
        long,
        long_help = "Set this to generate a report for the spam run(s) after spamming.",
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

    /// Max number of txs to send in a single json-rpc batch request.
    ///
    /// If set to 0 (default), contender sends one eth_sendRawTransaction request per tx.
    /// If set to N > 0, contender will group up to N txs into each json-rpc batch
    /// request (one http POST containing multiple eth_sendRawTransaction calls), while
    /// still sending the same total num of independent txs to the node.
    #[arg(
        long = "rpc-batch-size",
        value_name = "N",
        default_value_t = 0,
        long_help = "Max number of eth_sendRawTransaction calls to send in a single JSON-RPC batch request. \
                     0 (default) disables batching and sends one eth_sendRawTransaction per tx."
    )]
    pub rpc_batch_size: u64,

    #[arg(
        long = "timeout",
        long_help = "The time to wait for spammer to recover from failure before stopping contender. NOTE: this flag is deprecated and currently does nothing. It will be removed in a future release.",
        value_parser = parse_duration,
        default_value = "5min"
    )]
    pub spam_timeout: Duration,
}
#[derive(Clone)]
pub enum SpamScenario {
    Testfile(String),
    Builtin(BuiltinScenario),
}

impl SpamScenario {
    pub async fn testconfig(&self) -> Result<TestConfig> {
        use SpamScenario::*;
        let config: TestConfig = match self {
            Testfile(testfile) => load_testconfig(testfile).await?,
            Builtin(scenario) => scenario.to_owned().into(),
        };
        Ok(config)
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self, SpamScenario::Builtin(_))
    }
}

#[derive(Clone)]
pub struct SpamCommandArgs {
    pub scenario: SpamScenario,
    pub spam_args: SpamCliArgs,
    pub seed: RandSeed,
}

#[derive(Clone, Debug, Default)]
pub struct SpamCampaignContext {
    pub campaign_id: Option<String>,
    pub campaign_name: Option<String>,
    pub stage_name: Option<String>,
    pub scenario_name: Option<String>,
}

impl SpamCommandArgs {
    pub fn new(scenario: SpamScenario, cli_args: SpamCliArgs) -> Result<Self> {
        Ok(Self {
            scenario,
            spam_args: cli_args.clone(),
            seed: RandSeed::seed_from_str(
                &cli_args
                    .eth_json_rpc_args
                    .rpc_args
                    .seed
                    .unwrap_or(load_seedfile()?),
            ),
        })
    }

    pub async fn engine_params(&self) -> Result<EngineParams> {
        self.spam_args
            .eth_json_rpc_args
            .rpc_args
            .auth_args
            .engine_params(self.spam_args.eth_json_rpc_args.rpc_args.call_forkchoice)
            .await
    }

    pub async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
        &self,
        db: &D,
    ) -> Result<TestScenario<D, RandSeed, TestConfig>> {
        info!("Initializing spammer...");

        let SendSpamCliArgs {
            builder_url,
            txs_per_second,
            txs_per_block,
            duration,
            pending_timeout,
            run_forever,
        } = self.spam_args.spam_args.clone();
        let SendTxsCliArgsInner {
            min_balance,
            tx_type,
            bundle_type,
            env,
            override_senders,
            accounts_per_agent,
            ..
        } = self.spam_args.eth_json_rpc_args.rpc_args.clone();
        let accounts_per_agent = accounts_per_agent.unwrap_or(10);

        let mut testconfig = self.testconfig().await?;
        let spam_len = testconfig.spam.as_ref().map(|s| s.len()).unwrap_or(0);
        let txs_per_duration = txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam_len as u64));
        let engine_params = self.engine_params().await?;

        // Clamp rpc_batch_size to txs_per_duration (tps or tpb) if needed.
        let mut rpc_batch_size = self.spam_args.rpc_batch_size;
        if rpc_batch_size > 0 {
            if txs_per_duration == 0 {
                tracing::warn!(
                    "batch-rpc-num={} but there are no spam txs (txs_per_duration=0); disabling JSON-RPC batching",
                    rpc_batch_size
                );
                rpc_batch_size = 0;
            } else if rpc_batch_size > txs_per_duration {
                tracing::warn!(
                    "batch-rpc-num={} is greater than txs_per_duration={} (tps/tpb). Clamping batch-rpc-num to {}.",
                    rpc_batch_size,
                    txs_per_duration,
                    txs_per_duration
                );
                rpc_batch_size = txs_per_duration;
            }
        }

        if self.spam_args.redeploy && self.spam_args.skip_setup {
            return Err(RuntimeParamErrorKind::InvalidArgs(format!(
                "{} and {} cannot be passed together",
                bold("--redeploy"),
                bold("--skip-setup")
            ))
            .into());
        }

        // check if txs_per_duration is enough to cover the spam requests
        if (txs_per_duration * duration) < spam_len as u64 {
            return Err(ArgsError::TransactionsPerDurationInsufficient {
                min_tpd: spam_len as u64,
                tpd: txs_per_duration,
            }
            .into());
        }

        if let Some(spam) = &testconfig.spam {
            if spam.is_empty() {
                return Err(ArgsError::SpamNotFound.into());
            } else if builder_url.is_none() && spam.iter().any(|s| s.is_bundle()) {
                return Err(ArgsError::BuilderUrlRequiredForBundles.into());
            }

            // check tx types for non-builtin scenarios
            if !self.scenario.is_builtin() {
                // blobs
                if !spam
                    .iter()
                    .map(|sp| match sp {
                        SpamRequest::Bundle(_) => None,
                        SpamRequest::Tx(t) => t.blob_data.to_owned(),
                    })
                    .filter(|sp| sp.is_some())
                    .collect::<Vec<_>>()
                    .is_empty()
                    && tx_type != TxType::Eip4844
                {
                    return Err(ArgsError::TxTypeInvalid {
                        current_type: tx_type,
                        required_type: TxTypeCli::Eip4844,
                    }
                    .into());
                }

                // setCode txs
                if !spam
                    .iter()
                    .map(|sp| match sp {
                        SpamRequest::Bundle(_) => None,
                        SpamRequest::Tx(t) => t.authorization_address.to_owned(),
                    })
                    .filter(|sp| sp.is_some())
                    .collect::<Vec<_>>()
                    .is_empty()
                    && tx_type != TxType::Eip7702
                {
                    return Err(ArgsError::TxTypeInvalid {
                        current_type: tx_type,
                        required_type: TxTypeCli::Eip7702,
                    }
                    .into());
                }
            }
        }

        // Setup env variables
        let mut env_variables = testconfig.env.clone().unwrap_or_default();
        if let Some(env) = &env {
            env_variables.extend(env.iter().cloned());
        }
        testconfig.env = Some(env_variables.clone());

        let user_signers = self
            .spam_args
            .eth_json_rpc_args
            .rpc_args
            .user_signers_with_defaults();

        let num_setup_create_agents: usize = if self.scenario.is_builtin() { 1 } else { 0 };
        let agent_spec = AgentSpec::default()
            .create_accounts(num_setup_create_agents)
            .setup_accounts(num_setup_create_agents)
            .spam_accounts(accounts_per_agent as usize);

        let tx_type = match &self.scenario {
            SpamScenario::Builtin(builtin) => {
                if matches!(builtin, BuiltinScenario::Blobs(_)) {
                    TxType::Eip4844
                } else if matches!(builtin, BuiltinScenario::SetCode(_)) {
                    TxType::Eip7702
                } else {
                    tx_type.into()
                }
            }
            _ => tx_type.into(),
        };

        let rpc_client = self
            .spam_args
            .eth_json_rpc_args
            .rpc_args
            .new_rpc_provider()?;
        let block_time = get_block_time(&rpc_client).await?;

        check_private_keys(&testconfig, &user_signers);
        if txs_per_block.is_some() && txs_per_second.is_some() {
            panic!("Cannot set both --txs-per-block and --txs-per-second");
        }
        if txs_per_block.is_none() && txs_per_second.is_none() {
            panic!(
                "Must set either {} or {}",
                bold("--txs-per-block (--tpb)"),
                bold("--txs-per-second (--tps)")
            );
        }

        let agents = testconfig.build_agent_store(&self.seed, agent_spec.clone());

        if !override_senders {
            let all_signer_addrs = agents.all_signer_addresses();
            fund_accounts(
                &all_signer_addrs,
                &user_signers[0],
                &rpc_client,
                min_balance,
                TxType::Legacy,
                &engine_params,
            )
            .await?;
        }

        let done_fcu = Arc::new(AtomicBool::new(false));

        let fcu_handle = if let Some(auth_provider) = engine_params.engine_provider.to_owned() {
            let auth_provider = auth_provider.clone();
            let done_fcu = done_fcu.clone();
            Some(tokio::task::spawn(async move {
                loop {
                    if done_fcu.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    auth_provider.advance_chain(1).await?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<_, CliError>(())
            }))
        } else {
            None
        };

        let params = TestScenarioParams {
            rpc_url: self.spam_args.eth_json_rpc_args.rpc_args.rpc_url.clone(),
            builder_rpc_url: builder_url.to_owned(),
            signers: user_signers.to_owned(),
            agent_spec,
            tx_type,
            bundle_type: bundle_type.into(),
            pending_tx_timeout_secs: pending_timeout * block_time,
            extra_msg_handles: None,
            redeploy: self.spam_args.redeploy,
            sync_nonces_after_batch: !self.spam_args.optimistic_nonces,
            rpc_batch_size,
            gas_price: self.spam_args.eth_json_rpc_args.rpc_args.gas_price,
        };
        let mut test_scenario = TestScenario::new(
            testconfig,
            db.clone().into(),
            self.seed.clone(),
            params,
            engine_params.engine_provider.clone(),
            (&PROM, &HIST).into(),
        )
        .await?;

        // Builtin/default behavior: best-effort (skip redeploy if code exists); allow CLI override
        tracing::trace!(
            "spam mode: redeploy={} ({} ) [--redeploy flag set? {}]",
            self.spam_args.redeploy,
            if self.spam_args.redeploy {
                "will redeploy and run all setup"
            } else {
                "will skip redeploy when possible"
            },
            self.spam_args.redeploy
        );

        // run deployments & setup for builtin scenarios
        if self.scenario.is_builtin() && !self.spam_args.skip_setup {
            let test_scenario = &mut test_scenario;
            let setup_cost = test_scenario.estimate_setup_cost().await?;
            if min_balance < setup_cost {
                return Err(ArgsError::MinBalanceInsufficient {
                    min_balance,
                    required_balance: setup_cost,
                }
                .into());
            }
            tokio::select! {
                inner_res = async move {
                    if let Some(handle) = fcu_handle {
                        handle.await??;
                    } else {
                        // block until ctrl-c is pressed
                        tokio::signal::ctrl_c().await?;
                    }
                    Ok::<(), CliError>(())
                } => {
                    inner_res
                }
                inner_res = async move {
                    test_scenario.deploy_contracts().await?;
                    test_scenario.run_setup().await?;
                    Ok::<_, CliError>(())
                } => {
                    inner_res
                }
            }?;
        }
        done_fcu.store(true, std::sync::atomic::Ordering::SeqCst);

        // estimate spam cost. contracts must be deployed at this point,
        // otherwise you'll get "contract not found" errors
        let total_cost =
            U256::from(duration) * test_scenario.get_max_spam_cost(&user_signers).await?;
        if min_balance < U256::from(total_cost) {
            return Err(ArgsError::MinBalanceInsufficient {
                min_balance,
                required_balance: total_cost,
            }
            .into());
        }

        let duration_unit = if txs_per_second.is_some() {
            "second"
        } else {
            "block"
        };
        let duration_units = if duration > 1 {
            format!("{duration_unit}s")
        } else {
            duration_unit.to_owned()
        };
        if run_forever {
            warn!("Spammer agents will eventually run out of funds. Each batch of spam (sent over {duration} {duration_units}) will cost {} ETH.", format_ether(total_cost));
            // we use println! after warn! because warn! doesn't properly format bold strings
            println!(
                "Make sure you add plenty of funds with {} (set your pre-funded account with {}).",
                bold("spam --min-balance"),
                bold("spam -p"),
            );
        }

        Ok(test_scenario)
    }

    pub async fn testconfig(&self) -> Result<TestConfig> {
        self.spam_args
            .eth_json_rpc_args
            .rpc_args
            .testconfig(&self.scenario)
            .await
    }
}

pub fn spam_callback_default(
    log_txs: bool,
    send_fcu: bool,
    rpc_client: Option<Arc<AnyProvider>>,
    auth_client: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> TypedSpamCallback {
    if let Some(rpc_client) = rpc_client {
        if log_txs {
            let log_callback = LogCallback {
                rpc_provider: rpc_client.clone(),
                auth_provider: auth_client,
                send_fcu,
                cancel_token,
            };
            return TypedSpamCallback::Log(log_callback);
        }
    }
    TypedSpamCallback::Nil(NilCallback)
}

#[derive(Clone)]
pub enum TypedSpamCallback {
    Log(LogCallback),
    Nil(NilCallback),
}

impl TypedSpamCallback {
    pub fn is_log(&self) -> bool {
        matches!(self, TypedSpamCallback::Log(_))
    }
}

enum TypedSpammer {
    Blockwise(BlockwiseSpammer),
    Timed(TimedSpammer),
}

impl TypedSpammer {
    async fn spam_rpc<D, S, P>(
        &self,
        test_scenario: &mut TestScenario<D, S, P>,
        txs_per_period: u64,
        num_periods: u64,
        run_id: Option<u64>,
        tx_callback: TypedSpamCallback,
    ) -> Result<()>
    where
        D: DbOps + Clone + Send + Sync + 'static,
        S: SeedGenerator + Send + Sync + Clone,
        P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
    {
        macro_rules! spammit {
            ($spammer:expr, $tx_callback:expr) => {
                $spammer
                    .spam_rpc(
                        test_scenario,
                        txs_per_period,
                        num_periods,
                        run_id,
                        Arc::new($tx_callback),
                    )
                    .await?
            };
        }

        macro_rules! callback_match {
            ($spammer:expr) => {
                match tx_callback {
                    TypedSpamCallback::Log(tx_callback) => {
                        spammit!($spammer, tx_callback);
                    }
                    TypedSpamCallback::Nil(tx_callback) => {
                        spammit!($spammer, tx_callback);
                    }
                }
            };
        }

        match self {
            TypedSpammer::Blockwise(spammer) => callback_match!(spammer),
            TypedSpammer::Timed(spammer) => callback_match!(spammer),
        }
        Ok(())
    }
}

/// Spams given `test_scenario` and returns run ID, or None if using NilCallback & not part of a campaign.
pub async fn spam_inner<D, S, P>(
    db: &D,
    test_scenario: &mut TestScenario<D, S, P>,
    args: &SpamCommandArgs,
    run_context: SpamCampaignContext,
) -> Result<Option<u64>>
where
    D: DbOps + Clone + Send + Sync + 'static,
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    let start_block = test_scenario.rpc_client.get_block_number().await?;

    let SpamCommandArgs {
        scenario,
        spam_args,
        ..
    } = args;
    let SpamCliArgs {
        eth_json_rpc_args,
        spam_args,
        ignore_receipts,
        optimistic_nonces,
        ..
    } = spam_args.to_owned();
    let SendSpamCliArgs {
        txs_per_second,
        txs_per_block,
        duration,
        pending_timeout,
        run_forever,
        ..
    } = spam_args;
    let SendTxsCliArgsInner {
        auth_args,
        call_forkchoice,
        ..
    } = eth_json_rpc_args.rpc_args;
    let engine_params = auth_args.engine_params(call_forkchoice).await?;

    if run_forever && !optimistic_nonces {
        warn!("Notice: some transactions may fail when running the spammer indefinitely with nonce synchronization enabled.");
        eprintln!(
            "Setting {} without {} is likely to cause nonce synchronization errors in latter spam batches. Enable {} to avoid this.",
            bold("--forever"),
            bold("--optimistic-nonces"),
            bold("--optimistic-nonces")
        );
    }

    let mut run_id = None;
    let base_scenario_name = match scenario {
        SpamScenario::Testfile(testfile) => testfile.to_owned(),
        SpamScenario::Builtin(scenario) => scenario.title(),
    };
    let scenario_name = run_context
        .scenario_name
        .clone()
        .unwrap_or(base_scenario_name);
    let campaign_id = run_context.campaign_id.clone();
    let campaign_name = run_context.campaign_name.clone();
    let stage_name = run_context.stage_name.clone();

    let rpc_client = test_scenario.rpc_client.clone();
    let auth_client = test_scenario.auth_provider.to_owned();

    let block_time = get_block_time(&rpc_client).await?;

    use contender_core::Error as CCE;
    let err_parse = |err: CliError| match err {
        CliError::Core(m) => match m {
            CCE::Runtime(r) => match r {
                RuntimeErrorKind::InvalidParams(p) => match p {
                    RuntimeParamErrorKind::BundleTypeInvalid => ArgsError::BundleTypeInvalid.into(),
                    _ => p.into(),
                },
                _ => CliError::Core(contender_core::Error::Runtime(r)),
            },
            _ => m.into(),
        },
        _ => err,
    };

    let (spammer, txs_per_batch, spam_duration) = if let Some(txs_per_block) = txs_per_block {
        info!("Blockwise spammer starting. Sending {txs_per_block} txs per block.");
        (
            TypedSpammer::Blockwise(BlockwiseSpammer::new()),
            txs_per_block,
            SpamDuration::Blocks(duration),
        )
    } else if let Some(txs_per_second) = txs_per_second {
        info!("Timed spammer starting. Sending {txs_per_second} txs per second.");
        (
            TypedSpammer::Timed(TimedSpammer::new(std::time::Duration::from_secs(1))),
            txs_per_second,
            SpamDuration::Seconds(duration),
        )
    } else {
        return Err(ArgsError::SpamRateNotFound.into());
    };

    let callback = spam_callback_default(
        !ignore_receipts,
        engine_params.call_fcu,
        Some(rpc_client),
        auth_client,
        test_scenario.ctx.cancel_token.clone(),
    );

    let pending_timeout = Duration::from_secs(block_time * pending_timeout);
    if callback.is_log() || run_context.campaign_id.is_some() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let run = SpamRunRequest {
            timestamp: timestamp as usize,
            tx_count: (txs_per_batch * duration) as usize,
            scenario_name,
            campaign_id: campaign_id.clone(),
            campaign_name: campaign_name.clone(),
            stage_name: stage_name.clone(),
            rpc_url: test_scenario.rpc_url.to_string(),
            txs_per_duration: txs_per_batch,
            duration: spam_duration,
            pending_timeout,
        };
        run_id = Some(
            db.insert_run(&run)
                .map_err(|e| contender_core::Error::Db(e.into()))?, // TODO: revise this, we shouldn't need to use core errors here
        );
        if let Some(id) = run_id {
            info!(
                run_id = id,
                campaign_id = campaign_id.as_deref().unwrap_or(""),
                campaign_name = campaign_name.as_deref().unwrap_or(""),
                stage = stage_name.as_deref().unwrap_or(""),
                "Created spam run"
            );
        }
    }

    // initialize TxActor (pending tx cache processor) context
    let actor_ctx = ActorContext::new(start_block, run_id.unwrap_or_default())
        .with_pending_tx_timeout(pending_timeout);
    test_scenario.tx_actor().init_ctx(actor_ctx).await?;

    // loop spammer, break if CTRL-C is received, or run_forever is false
    loop {
        tokio::select! {
            res = {
                spammer
                .spam_rpc(
                    test_scenario,
                    txs_per_batch,
                    duration,
                    run_id,
                    callback.clone(),
                )
            } => {
                res.map_err(err_parse)?;
            }

            _ = tokio::signal::ctrl_c() => {
                println!("\nCTRL-C received, shutting down spam run.");
                test_scenario.shutdown().await;
            }
        }

        if !run_forever || test_scenario.is_shutdown().await {
            break;
        }
    }

    // wait for tx results, or break for CTRL-C
    tokio::select! {
        _ = test_scenario.dump_tx_cache(run_id.unwrap_or_default()) => {
        }

        _ = tokio::signal::ctrl_c() => {
            info!("CTRL-C received, stopping result collection.");
            test_scenario.shutdown().await;
        }
    }

    Ok(run_id)
}

/// Runs spammer and returns run ID.
pub async fn spam<D: DbOps + Clone + Send + Sync + 'static>(
    db: &D,
    args: &SpamCommandArgs,
    run_context: SpamCampaignContext,
) -> Result<Option<u64>> {
    let mut test_scenario = args.init_scenario(db).await?;
    spam_inner(db, &mut test_scenario, args, run_context).await
}

#[cfg(test)]
mod tests {
    use crate::commands::common::AuthCliArgs;
    use crate::commands::SetupCommandArgs;

    use super::*;
    use alloy::node_bindings::{Anvil, AnvilInstance, WEI_IN_ETHER};
    use contender_sqlite::SqliteDb;
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
    };

    fn create_send_args(sf: &Path, anvil: &AnvilInstance) -> ScenarioSendTxsCliArgs {
        // map scenario files to custom tx types if needed
        // this might be replaced with a more robust solution in the future
        // e.g. mapping the entire ScenarioSendTxsCliArgs structure instead of just tx types
        let custom_tx_types = HashMap::<&str, TxTypeCli>::from_iter([
            ("blobs.toml", TxTypeCli::Eip4844),
            ("setCode.toml", TxTypeCli::Eip7702),
        ]);

        // use last components of the path after "scenarios/" as a scenario ID
        // this supports nested directories under "scenarios/"
        let relative_path = sf
            .components()
            .rev()
            .take_while(|component| component.as_os_str() != "scenarios")
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<PathBuf>();

        ScenarioSendTxsCliArgs {
            testfile: Some(sf.to_str().unwrap().to_owned()),
            rpc_args: SendTxsCliArgsInner {
                rpc_url: anvil.endpoint_url(),
                seed: None,
                private_keys: None,
                min_balance: WEI_IN_ETHER * U256::from(10),
                tx_type: custom_tx_types
                    .get(relative_path.as_os_str().to_str().unwrap())
                    .cloned()
                    .unwrap_or(TxTypeCli::Eip1559),
                bundle_type: crate::commands::common::BundleTypeCli::L1,
                auth_args: AuthCliArgs::default(),
                call_forkchoice: false,
                env: None,
                override_senders: false,
                gas_price: None,
                accounts_per_agent: None,
            },
        }
    }

    async fn run_scenario(
        sf: &Path,
        anvil: &AnvilInstance,
        db: &SqliteDb,
        rand_seed: &RandSeed,
    ) -> Result<()> {
        // special case: skip bundle scenario (anvil doesn't support it)
        if sf.ends_with("bundles.toml") {
            println!("Skipping bundle scenario (anvil doesn't support bundles)");
            return Ok(());
        }

        // initialize a logger
        let _ = tracing_subscriber::fmt()
            .with_env_filter("contender_core=debug,info")
            .with_test_writer() // captures output properly in tests
            .try_init(); // try_init() won't panic if already initialized

        // initialize scenario
        let scenario = SpamScenario::Testfile(sf.to_str().unwrap().to_owned());
        let send_args = create_send_args(sf, anvil);

        // run setup
        crate::commands::setup(
            db,
            SetupCommandArgs {
                scenario: scenario.clone(),
                eth_json_rpc_args: send_args.rpc_args.clone(),
                seed: rand_seed.clone(),
            },
        )
        .await?;

        // do a quick spam run
        let res = spam(
            db,
            &SpamCommandArgs {
                scenario,
                spam_args: SpamCliArgs {
                    eth_json_rpc_args: send_args,
                    spam_args: SendSpamCliArgs {
                        builder_url: None,
                        txs_per_second: Some(50),
                        txs_per_block: None,
                        duration: 4,
                        pending_timeout: 10,
                        run_forever: false,
                    },
                    ignore_receipts: false,
                    optimistic_nonces: true,
                    gen_report: false,
                    redeploy: true,
                    skip_setup: false,
                    rpc_batch_size: 0,
                    spam_timeout: Duration::from_secs(5),
                },
                seed: rand_seed.clone(),
            },
            SpamCampaignContext {
                campaign_id: None,
                campaign_name: None,
                stage_name: None,
                scenario_name: None,
            },
        )
        .await?;

        println!("spam run successful. run id: {:?}", res);
        Ok(())
    }

    /// Spin up a fresh anvil instance, DB, & seed, then run the scenario file given at `path`.
    async fn run_scenario_file(path: &Path) -> Result<()> {
        let anvil = Anvil::new().block_time(1).spawn();
        let db = SqliteDb::new_memory();
        db.create_tables()?;
        let rand_seed = RandSeed::new();

        run_scenario(path, &anvil, &db, &rand_seed).await
    }

    /// Generates individual spam test for each scenario given in the macro input.
    /// NOTE: paths are relative to the project root. See `build.rs` for usage.
    macro_rules! scenario_tests {
        ($($name:ident => $relative_path:expr),* $(,)?) => {
            $(
                #[tokio::test]
                async fn $name() -> std::result::Result<(), CliError> {
                    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap();
                    let path: PathBuf = project_root.join($relative_path);
                    run_scenario_file(&path).await?;
                    Ok(())
                }
            )*
        };
    }

    #[allow(non_snake_case)]
    mod generated_scenario_tests {
        use super::*;
        // Generate tests for all scenario files identified by build.rs
        include!(concat!(env!("OUT_DIR"), "/generated_scenario_tests.rs"));
    }
}
