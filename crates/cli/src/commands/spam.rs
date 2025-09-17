use super::common::{ScenarioSendTxsCliArgs, SendSpamCliArgs};
use crate::{
    commands::common::EngineParams,
    default_scenarios::BuiltinScenario,
    util::{
        bold, check_private_keys, fund_accounts, get_signers_with_defaults, load_seedfile,
        load_testconfig, parse_duration, provider::AuthClient, spam_callback_default,
        TypedSpamCallback,
    },
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::{utils::format_ether, U256},
    providers::{DynProvider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::AgentStore,
    db::{DbOps, SpamDuration, SpamRunRequest},
    error::{ContenderError, RuntimeParamErrorKind},
    generator::{seeder::Seeder, templater::Templater, types::SpamRequest, PlanConfig, RandSeed},
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
    util::get_block_time,
};
use contender_engine_provider::{
    reth_node_api::EngineApiMessageVersion, AdvanceChain, AuthProvider,
};
use contender_testfile::TestConfig;
use op_alloy_network::Optimism;
use std::{ops::Deref, path::PathBuf, sync::atomic::AtomicBool};
use std::{sync::Arc, time::Duration};
use tracing::{info, warn};

#[derive(Debug)]
pub struct EngineArgs {
    pub auth_rpc_url: String,
    pub jwt_secret: PathBuf,
    pub use_op: bool,
    pub message_version: EngineApiMessageVersion,
}

impl EngineArgs {
    pub async fn new_provider(&self) -> Result<AuthClient, Box<dyn std::error::Error>> {
        let provider: Box<dyn AdvanceChain + Send + Sync + 'static> = if self.use_op {
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
                AuthProvider::<AnyNetwork>::from_jwt_file(
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
            long_help = "Prevent tx results from being saved to DB.",
            visible_aliases = &["dr"]
        )]
    pub disable_reporting: bool,

    #[arg(
        long,
        long_help = "Set this to generate a report for the spam run(s) after spamming.",
        visible_aliases = &["report"]
    )]
    pub gen_report: bool,

    #[arg(
        long = "timeout",
        long_help = "The time to wait for spammer to recover from failure before stopping contender.",
        value_parser = parse_duration,
        default_value = "5min"
    )]
    pub spam_timeout: Duration,

    /// Re-deploy contracts in builtin scenarios.
    #[arg(
        long,
        long_help = "If set, re-deploy contracts that have already been deployed. Only builtin scenarios are affected."
    )]
    pub redeploy: bool,
}

pub enum SpamScenario {
    Testfile(String),
    Builtin(BuiltinScenario),
}

impl SpamScenario {
    pub async fn testconfig(&self) -> Result<TestConfig, ContenderError> {
        let config: TestConfig = match self {
            SpamScenario::Testfile(testfile) => load_testconfig(testfile)
                .await
                .map_err(|e| ContenderError::with_err(e.deref(), "failed to load testconfig"))?,
            SpamScenario::Builtin(scenario) => scenario.to_owned().into(),
        };
        Ok(config)
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self, SpamScenario::Builtin(_))
    }
}

pub struct SpamCommandArgs {
    pub scenario: SpamScenario,
    pub spam_args: SpamCliArgs,
    pub seed: RandSeed,
}

impl SpamCommandArgs {
    pub fn new(scenario: SpamScenario, cli_args: SpamCliArgs) -> contender_core::Result<Self> {
        Ok(Self {
            scenario,
            spam_args: cli_args.clone(),
            seed: RandSeed::seed_from_str(
                &cli_args.eth_json_rpc_args.seed.unwrap_or(
                    load_seedfile().map_err(|e| {
                        ContenderError::with_err(e.deref(), "failed to load seedfile")
                    })?,
                ),
            ),
        })
    }

    async fn engine_params(&self) -> contender_core::Result<EngineParams> {
        self.spam_args
            .eth_json_rpc_args
            .auth_args
            .engine_params()
            .await
            .map_err(|e| ContenderError::with_err(e.deref(), "failed to build engine params"))
    }

    pub async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
        &self,
        db: &D,
    ) -> Result<TestScenario<D, RandSeed, TestConfig>, ContenderError> {
        info!("Initializing spammer...");

        let SendSpamCliArgs {
            builder_url,
            txs_per_second,
            txs_per_block,
            duration,
            pending_timeout,
            loops,
            accounts_per_agent,
        } = self.spam_args.spam_args.clone();
        let ScenarioSendTxsCliArgs {
            rpc_url,
            private_keys,
            min_balance,
            tx_type,
            bundle_type,
            env,
            ..
        } = self.spam_args.eth_json_rpc_args.clone();

        let url = Url::parse(&rpc_url).expect("Invalid RPC URL");
        let rpc_client = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_http(url.to_owned()),
        );

        let mut testconfig = self.scenario.testconfig().await?;
        let spam_len = testconfig.spam.as_ref().map(|s| s.len()).unwrap_or(0);
        let txs_per_duration = txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam_len as u64));
        let block_time = get_block_time(&rpc_client).await?;
        let engine_params = self.engine_params().await?;

        // check if txs_per_duration is enough to cover the spam requests
        if txs_per_duration < spam_len as u64 {
            return Err(ContenderError::SpamError(
                "Not enough transactions per duration to cover spam requests.",
                Some(format!(
                    "Set {} or {} to at least {spam_len}",
                    bold("--txs-per-block (--tpb)"),
                    bold("--txs-per-second (--tps)"),
                )),
            ));
        }

        if let Some(spam) = &testconfig.spam {
            if spam.is_empty() {
                return Err(ContenderError::SpamError(
                    "No spam calls found in testfile",
                    None,
                ));
            } else if builder_url.is_none() && spam.iter().any(|s| s.is_bundle()) {
                return Err(ContenderError::SpamError(
                    "Builder URL is required to send bundles.",
                    Some(format!(
                        "Pass the builder's URL with {}",
                        bold("--builder-url <URL>")
                    )),
                ));
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
                    return Err(ContenderError::SpamError(
                        "invalid tx type for blob transactions.",
                        Some(format!("must set tx type {}", bold("-t eip4844"))),
                    ));
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
                    return Err(ContenderError::SpamError(
                        "invalid tx type for setCode transactions.",
                        Some(format!("must set tx type {}", bold("-t eip7702"))),
                    ));
                }
            }
        }

        // Setup env variables
        let mut env_variables = testconfig.env.clone().unwrap_or_default();
        if let Some(env) = &env {
            env_variables.extend(env.iter().cloned());
        }
        testconfig.env = Some(env_variables.clone());

        let user_signers = get_signers_with_defaults(private_keys.to_owned());

        // distill all from_pool arguments from the spam requests
        let from_pool_declarations = testconfig.get_spam_pools();

        let mut agents = AgentStore::new();
        agents.init(
            &from_pool_declarations,
            accounts_per_agent as usize,
            &self.seed,
        );

        if self.scenario.is_builtin() {
            agents.init(
                &[testconfig.get_create_pools(), testconfig.get_setup_pools()].concat(),
                1,
                &self.seed,
            );
        }

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

        let all_signer_addrs = agents.all_signer_addresses();

        let params = TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: builder_url
                .to_owned()
                .map(|url| Url::parse(&url).expect("Invalid builder URL")),
            signers: user_signers.to_owned(),
            agent_store: agents.to_owned(),
            tx_type,
            bundle_type: bundle_type.into(),
            pending_tx_timeout_secs: pending_timeout * block_time,
            extra_msg_handles: None,
            redeploy: self.spam_args.redeploy,
        };

        fund_accounts(
            &all_signer_addrs,
            &user_signers[0],
            &rpc_client,
            min_balance,
            TxType::Legacy,
            &engine_params,
        )
        .await
        .map_err(|e| ContenderError::with_err(e.deref(), "failed to fund accounts"))?;

        let done_fcu = Arc::new(AtomicBool::new(false));

        let fcu_handle = if let Some(auth_provider) = engine_params.engine_provider.to_owned() {
            let auth_provider = auth_provider.clone();
            let done_fcu = done_fcu.clone();
            Some(tokio::task::spawn(async move {
                loop {
                    if done_fcu.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    auth_provider
                        .advance_chain(1)
                        .await
                        .map_err(|e| ContenderError::with_err(e, "failed to advance chain"))?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<_, ContenderError>(())
            }))
        } else {
            None
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
        if self.scenario.is_builtin() {
            let test_scenario = &mut test_scenario;
            let setup_cost = test_scenario.estimate_setup_cost().await?;
            if min_balance < setup_cost {
                return Err(ContenderError::SpamError(
                    "min_balance is not enough to cover the cost of the setup transactions.",
                    format!(
                        "min_balance: {}, setup_cost: {}\nUse {} to increase the amount of funds sent to agent wallets.",
                        format_ether(min_balance),
                        format_ether(setup_cost),
                        bold("spam --min-balance <ETH amount>"),
                    )
                    .into(),
                ));
            }
            tokio::select! {
                inner_res = async move {
                    if let Some(handle) = fcu_handle {
                        handle.await.map_err(|e| ContenderError::with_err(e, "failed to join fcu task"))??;
                    } else {
                        // block until ctrl-c is pressed
                        tokio::signal::ctrl_c().await.map_err(|e| ContenderError::with_err(e, "failed to wait for ctrl-c"))?;
                    }
                    Ok::<(), ContenderError>(())
                } => {
                    inner_res
                }
                inner_res = async move {
                    test_scenario.deploy_contracts().await?;
                    test_scenario.run_setup().await?;
                    Ok::<_, ContenderError>(())
                } => {
                    inner_res
                }
            }?;
        }
        done_fcu.store(true, std::sync::atomic::Ordering::SeqCst);

        if loops.is_some_and(|inner_loops| inner_loops.is_none()) {
            warn!("Spammer agents will eventually run out of funds.");
            println!(
                "Make sure you add plenty of funds with {} (set your pre-funded account with {}).",
                bold("spam --min-balance"),
                bold("spam -p"),
            );
        }

        let total_cost = U256::from(duration * loops.flatten().unwrap_or(1))
            * test_scenario.get_max_spam_cost(&user_signers).await?;
        if min_balance < U256::from(total_cost) {
            return Err(ContenderError::SpamError(
                "min_balance is not enough to cover the cost of the spam transactions.",
                format!(
                    "min_balance: {}, total_cost: {}\nUse {} to increase the amount of funds sent to agent wallets.",
                    format_ether(min_balance),
                    format_ether(total_cost),
                    bold("spam --min-balance <ETH amount>"),
                )
                .into(),
            ));
        }

        Ok(test_scenario)
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
    ) -> Result<(), ContenderError>
    where
        D: DbOps + Clone + Send + Sync + 'static,
        S: Seeder + Send + Sync + Clone,
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

/// Runs spammer and returns run ID.
pub async fn spam<
    D: DbOps + Clone + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
>(
    db: &D,
    args: &SpamCommandArgs,
    test_scenario: &mut TestScenario<D, S, P>,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let SpamCommandArgs {
        scenario,
        spam_args,
        ..
    } = args;
    let SpamCliArgs {
        eth_json_rpc_args,
        spam_args,
        disable_reporting,
        ..
    } = spam_args.to_owned();
    let SendSpamCliArgs {
        txs_per_second,
        txs_per_block,
        duration,
        pending_timeout,
        ..
    } = spam_args;
    let ScenarioSendTxsCliArgs { auth_args, .. } = eth_json_rpc_args;
    let engine_params = auth_args
        .engine_params()
        .await
        .map_err(|e| ContenderError::with_err(e.deref(), "failed to build engine params"))?;

    let mut run_id = None;
    let scenario_name = match scenario {
        SpamScenario::Testfile(testfile) => testfile.to_owned(),
        SpamScenario::Builtin(scenario) => scenario.title(),
    };

    let rpc_client = test_scenario.rpc_client.clone();
    let auth_client = test_scenario.auth_provider.to_owned();

    let block_time = get_block_time(&rpc_client).await?;

    let err_parse = |e: ContenderError| match e {
        ContenderError::InvalidRuntimeParams(kind) => match kind {
            RuntimeParamErrorKind::BundleTypeInvalid => ContenderError::SpamError(
                "Invalid bundle type.",
                Some(format!(
                    "Set a different bundle type with {}",
                    bold("--bundle-type")
                )),
            ),
            err => err.into(),
        },
        err => err,
    };

    let (spammer, txs_per_batch) = if let Some(txs_per_block) = txs_per_block {
        info!("Blockwise spammer starting. Sending {txs_per_block} txs per block.");
        (
            TypedSpammer::Blockwise(BlockwiseSpammer::new()),
            txs_per_block,
        )
    } else if let Some(txs_per_second) = txs_per_second {
        info!("Timed spammer starting. Sending {txs_per_second} txs per second.");
        (
            TypedSpammer::Timed(TimedSpammer::new(std::time::Duration::from_secs(1))),
            txs_per_second,
        )
    } else {
        return Err(Box::new(ContenderError::SpamError(
            "Missing params.",
            Some(format!(
                "Either {} or {} must be set.",
                bold("--txs-per-block"),
                bold("--txs-per-second"),
            )),
        )));
    };

    let callback = spam_callback_default(
        !disable_reporting,
        engine_params.call_fcu,
        Some(rpc_client),
        auth_client,
        test_scenario.ctx.cancel_token.clone(),
    );

    if callback.is_log() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        let run = SpamRunRequest {
            timestamp: timestamp as usize,
            tx_count: (txs_per_batch * duration) as usize,
            scenario_name,
            rpc_url: test_scenario.rpc_url.to_string(),
            txs_per_duration: txs_per_batch,
            duration: SpamDuration::Blocks(duration),
            pending_timeout: Duration::from_secs(block_time * pending_timeout),
        };
        run_id = Some(db.insert_run(&run)?);
    }

    spammer
        .spam_rpc(test_scenario, txs_per_batch, duration, run_id, callback)
        .await
        .map_err(err_parse)?;

    Ok(run_id)
}
