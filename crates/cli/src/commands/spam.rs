use super::common::{ScenarioSendTxsCliArgs, SendSpamCliArgs};
use crate::{
    default_scenarios::BuiltinScenario,
    util::{
        check_private_keys, fund_accounts, get_signers_with_defaults, load_testconfig,
        provider::AuthClient, spam_callback_default, EngineParams, SpamCallbackType,
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
    generator::{seeder::Seeder, templater::Templater, PlanConfig, RandSeed},
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
    BundleType,
};
use contender_engine_provider::{AdvanceChain, AuthProvider};
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
}

impl EngineArgs {
    pub async fn new_provider(&self) -> Result<AuthClient, Box<dyn std::error::Error>> {
        let provider: Box<dyn AdvanceChain + Send + Sync + 'static> = if self.use_op {
            Box::new(
                AuthProvider::<Optimism>::from_jwt_file(&self.auth_rpc_url, &self.jwt_secret)
                    .await?,
            )
        } else {
            Box::new(
                AuthProvider::<AnyNetwork>::from_jwt_file(&self.auth_rpc_url, &self.jwt_secret)
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
            SpamScenario::Builtin(scenario) => TestConfig::from(scenario.to_owned()),
        };
        Ok(config)
    }

    pub fn is_builtin(&self) -> bool {
        match self {
            SpamScenario::Testfile(_) => false,
            SpamScenario::Builtin(_) => true,
        }
    }
}

pub struct SpamCommandArgs {
    pub scenario: SpamScenario,
    pub rpc_url: String,
    pub builder_url: Option<String>,
    pub txs_per_block: Option<u64>,
    pub txs_per_second: Option<u64>,
    pub duration: u64,
    pub seed: String,
    pub private_keys: Option<Vec<String>>,
    pub disable_reporting: bool,
    pub min_balance: U256,
    pub tx_type: TxType,
    pub bundle_type: BundleType,
    /// Provide to enable engine calls (required to use `call_forkchoice`)
    pub engine_params: EngineParams,
    pub timeout_secs: u64,
    pub env: Option<Vec<(String, String)>>,
    pub loops: Option<u64>,
}

impl SpamCommandArgs {
    pub async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
        &self,
        db: &D,
    ) -> Result<TestScenario<D, RandSeed, TestConfig>, ContenderError> {
        info!("Initializing spammer...");
        let SpamCommandArgs {
            txs_per_block,
            txs_per_second,
            scenario,
            duration,
            seed,
            rpc_url,
            builder_url,
            min_balance,
            private_keys,
            tx_type,
            bundle_type,
            timeout_secs,
            engine_params,
            loops,
            ..
        } = self;

        let mut testconfig = scenario.testconfig().await?;
        let spam_len = testconfig.spam.as_ref().map(|s| s.len()).unwrap_or(0);

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
                        ansi_term::Style::new().bold().paint("--builder-url <URL>")
                    )),
                ));
            }
        }

        // Setup env variables
        let mut env_variables = testconfig.env.clone().unwrap_or_default();
        if let Some(env) = &self.env {
            env_variables.extend(env.iter().cloned());
        }
        testconfig.env = Some(env_variables.clone());

        let rand_seed = RandSeed::seed_from_str(seed);
        let url = Url::parse(rpc_url).expect("Invalid RPC URL");
        let rpc_client = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .connect_http(url.to_owned()),
        );

        let user_signers = get_signers_with_defaults(private_keys.to_owned());

        // distill all from_pool arguments from the spam requests
        let from_pool_declarations = testconfig.get_spam_pools();

        let mut agents = AgentStore::new();
        let txs_per_duration = txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam_len as u64));
        let signers_per_period = txs_per_duration / from_pool_declarations.len().max(1) as u64;
        agents.init(
            &from_pool_declarations,
            signers_per_period as usize,
            &rand_seed,
        );
        if self.scenario.is_builtin() {
            agents.init(
                &[testconfig.get_create_pools(), testconfig.get_setup_pools()].concat(),
                1,
                &rand_seed,
            );
        }

        let all_agents = agents.all_agents().collect::<Vec<_>>();
        if (txs_per_duration as usize) < all_agents.len() {
            return Err(ContenderError::SpamError(
                "Not enough signers to cover all agent pools. Set --tps or --tpb to a higher value.",
                Some(format!(
                    "signers_per_period: {}, agents: {}",
                    signers_per_period,
                    all_agents.len()
                )),
            ));
        }

        check_private_keys(&testconfig, &user_signers);

        if txs_per_block.is_some() && txs_per_second.is_some() {
            panic!("Cannot set both --txs-per-block and --txs-per-second");
        }
        if txs_per_block.is_none() && txs_per_second.is_none() {
            panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
        }

        let all_signer_addrs = agents.all_signer_addresses();

        let params = TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: builder_url
                .to_owned()
                .map(|url| Url::parse(&url).expect("Invalid builder URL")),
            signers: user_signers.to_owned(),
            agent_store: agents.to_owned(),
            tx_type: *tx_type,
            bundle_type: *bundle_type,
            pending_tx_timeout_secs: *timeout_secs,
        };

        fund_accounts(
            &all_signer_addrs,
            &user_signers[0],
            &rpc_client,
            *min_balance,
            *tx_type,
            engine_params,
        )
        .await
        .map_err(|e| ContenderError::with_err(e.deref(), "failed to fund accounts"))?;

        let done_fcu = Arc::new(AtomicBool::new(false));
        let (error_sender, mut error_receiver) = tokio::sync::mpsc::channel::<String>(1);
        let error_sender = Arc::new(error_sender);

        if let Some(auth_provider) = engine_params.engine_provider.to_owned() {
            let auth_provider = auth_provider.clone();
            let done_fcu = done_fcu.clone();
            let error_sender = error_sender.clone();
            tokio::task::spawn(async move {
                loop {
                    if done_fcu.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }

                    let mut err = String::new();
                    auth_provider.advance_chain(1).await.unwrap_or_else(|e| {
                        err = e.to_string();
                    });
                    if !err.is_empty() {
                        error_sender
                            .send(err)
                            .await
                            .expect("failed to send error from task");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            });
        };

        let mut scenario = TestScenario::new(
            testconfig,
            db.clone().into(),
            rand_seed,
            params,
            engine_params.engine_provider.to_owned(),
            (&PROM, &HIST),
        )
        .await?;

        if self.scenario.is_builtin() {
            let scenario = &mut scenario;
            let setup_res = tokio::select! {
                Some(err) = error_receiver.recv() => {
                    Err(err)
                }
                res = async move {
                    let str_err = |e| format!("Setup error: {e}");
                    scenario.deploy_contracts().await.map_err(str_err)?;
                    scenario.run_setup().await.map_err(str_err)?;
                    Ok::<(), String>(())
                } => {
                    res
                }
            };
            if let Err(e) = setup_res {
                return Err(ContenderError::SpamError(
                    "Builtin scenario setup failed.",
                    Some(e),
                ));
            }
        }
        done_fcu.store(true, std::sync::atomic::Ordering::SeqCst);

        if loops.is_none() {
            warn!(
                "Spammer agents will eventually run out of funds. Make sure you add plenty of funds with {} (set your pre-funded account with {}).",
                ansi_term::Style::new().bold().paint("spam --min-balance"),
                ansi_term::Style::new().bold().paint("spam -p"),
            );
        }

        let total_cost = U256::from(duration * loops.unwrap_or(1))
            * scenario.get_max_spam_cost(&user_signers).await?;
        if *min_balance < U256::from(total_cost) {
            return Err(ContenderError::SpamError(
                "min_balance is not enough to cover the cost of the spam transactions.",
                format!(
                    "min_balance: {}, total_cost: {}\nUse {} to increase the amount of funds sent to agent wallets.",
                    format_ether(*min_balance),
                    format_ether(total_cost),
                    ansi_term::Style::new()
                        .bold()
                        .paint("spam --min-balance <ETH amount>"),
                )
                .into(),
            ));
        }

        Ok(scenario)
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
        txs_per_block,
        txs_per_second,
        scenario,
        duration,
        disable_reporting,
        engine_params,
        timeout_secs,
        ..
    } = args;

    let mut run_id = None;
    let scenario_name = match scenario {
        SpamScenario::Testfile(testfile) => testfile.to_owned(),
        SpamScenario::Builtin(scenario) => scenario.to_string(),
    };

    let rpc_client = test_scenario.rpc_client.clone();
    let auth_client = test_scenario.auth_provider.to_owned();

    let err_parse = |e: ContenderError| match e {
        ContenderError::InvalidRuntimeParams(kind) => match kind {
            RuntimeParamErrorKind::BundleTypeInvalid => ContenderError::SpamError(
                "Invalid bundle type.",
                Some(format!(
                    "Set a different bundle type with {}",
                    ansi_term::Style::new().bold().paint("--bundle-type")
                )),
            ),
            err => err.into(),
        },
        err => err,
    };

    // trigger blockwise spammer
    if let Some(txs_per_block) = txs_per_block {
        info!("Blockwise spamming with {txs_per_block} txs per block");
        let spammer = BlockwiseSpammer::new();

        match spam_callback_default(
            !disable_reporting,
            engine_params.call_fcu,
            Some(rpc_client.clone()),
            auth_client,
            test_scenario.ctx.cancel_token.clone(),
        )
        .await
        {
            SpamCallbackType::Log(tx_callback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                let run = SpamRunRequest {
                    timestamp: timestamp as usize,
                    tx_count: (*txs_per_block * duration) as usize,
                    scenario_name,
                    rpc_url: test_scenario.rpc_url.to_string(),
                    txs_per_duration: *txs_per_block,
                    duration: SpamDuration::Blocks(*duration),
                    timeout: *timeout_secs,
                };
                run_id = Some(db.insert_run(&run)?);
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        *duration,
                        run_id,
                        tx_callback.into(),
                    )
                    .await
                    .map_err(err_parse)?;
            }
            SpamCallbackType::Nil(tx_callback) => {
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        *duration,
                        None,
                        tx_callback.into(),
                    )
                    .await
                    .map_err(err_parse)?;
            }
        };
        return Ok(run_id);
    }

    // trigger timed spammer
    let tps = txs_per_second.unwrap_or(10);
    info!("Timed spamming with {tps} txs per second");
    let spammer = TimedSpammer::new(std::time::Duration::from_secs(1));
    match spam_callback_default(
        !disable_reporting,
        engine_params.call_fcu,
        rpc_client.into(),
        auth_client,
        test_scenario.ctx.cancel_token.clone(),
    )
    .await
    {
        SpamCallbackType::Log(tx_callback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            let run = SpamRunRequest {
                timestamp: timestamp as usize,
                tx_count: (tps * duration) as usize,
                scenario_name,
                rpc_url: test_scenario.rpc_url.to_string(),
                txs_per_duration: tps,
                duration: SpamDuration::Seconds(*duration),
                timeout: *timeout_secs,
            };
            run_id = Some(db.insert_run(&run)?);
            spammer
                .spam_rpc(test_scenario, tps, *duration, run_id, tx_callback.into())
                .await
                .map_err(err_parse)?;
        }
        SpamCallbackType::Nil(tx_callback) => {
            spammer
                .spam_rpc(test_scenario, tps, *duration, None, tx_callback.into())
                .await
                .map_err(err_parse)?;
        }
    };

    Ok(run_id)
}
