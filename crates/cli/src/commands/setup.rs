use super::common::ScenarioSendTxsCliArgs;
use crate::{
    commands::{common::EngineParams, SpamScenario},
    util::{
        check_private_keys_fns, find_insufficient_balances, fund_accounts,
        get_signers_with_defaults, load_seedfile,
    },
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    network::AnyNetwork,
    primitives::utils::format_ether,
    providers::{DynProvider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    error::ContenderError,
    generator::RandSeed,
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_core::{generator::PlanConfig, util::get_block_time};
use contender_engine_provider::DEFAULT_BLOCK_TIME;
use std::{
    ops::Deref,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub async fn setup(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    args: SetupCommandArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let ScenarioSendTxsCliArgs {
        rpc_url,
        private_keys,
        min_balance,
        tx_type,
        env,
        bundle_type,
        ..
    } = args.eth_json_rpc_args.clone();
    let engine_params = args.engine_params().await?;

    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(url.to_owned()),
    );
    let mut testconfig = args.scenario.testconfig().await?;

    // Setup env variables
    let mut env_variables = testconfig.env.clone().unwrap_or_default();
    if let Some(env) = env {
        for (key, value) in env {
            let _ = &env_variables.insert(key.to_string(), value.to_string());
        }
    }
    testconfig.env = Some(env_variables.clone());

    let user_signers = private_keys
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|key| PrivateKeySigner::from_str(key).expect("invalid private key"))
        .collect::<Vec<PrivateKeySigner>>();

    let user_signers_with_defaults = get_signers_with_defaults(private_keys);

    check_private_keys_fns(
        &testconfig.setup.to_owned().unwrap_or_default(),
        &user_signers_with_defaults,
    );

    // ensure user-provided accounts have sufficient balance
    let broke_accounts = find_insufficient_balances(
        &user_signers.iter().map(|s| s.address()).collect::<Vec<_>>(),
        min_balance,
        &rpc_client,
    )
    .await?;
    if !broke_accounts.is_empty() {
        return Err(ContenderError::SetupError(
            "Insufficient balance in provided user account(s).",
            Some(format!(
                "{:?}",
                broke_accounts
                    .iter()
                    .map(|(addr, bal)| format!("{}: {} ETH", addr, format_ether(*bal)))
                    .collect::<Vec<_>>()
            )),
        )
        .into());
    }

    // load agents from setup and create pools
    let from_pool_declarations =
        [testconfig.get_setup_pools(), testconfig.get_create_pools()].concat();

    // create agents for each from_pool declaration
    let mut agents = AgentStore::new();
    for from_pool in &from_pool_declarations {
        if agents.has_agent(from_pool) {
            continue;
        }

        let agent = SignerStore::new(1, &args.seed, from_pool);
        agents.add_agent(from_pool, agent);
    }

    // user-provided signers must be pre-funded
    let admin_signer = user_signers_with_defaults[0].to_owned();
    let all_agent_addresses = agents.all_signer_addresses();

    let params = TestScenarioParams {
        rpc_url: url,
        builder_rpc_url: None,
        signers: user_signers_with_defaults,
        agent_store: agents,
        tx_type: tx_type.into(),
        bundle_type: bundle_type.into(),
        pending_tx_timeout_secs: 12,
        extra_msg_handles: None,
        redeploy: true,
        sync_nonces_after_batch: true,
    };

    fund_accounts(
        &all_agent_addresses,
        &admin_signer,
        &rpc_client,
        min_balance,
        tx_type.into(),
        &engine_params,
    )
    .await?;

    let mut scenario = TestScenario::new(
        testconfig.to_owned(),
        db.clone().into(),
        args.seed.clone(),
        params,
        engine_params.engine_provider,
        (&PROM, &HIST).into(),
    )
    .await?;

    let total_cost = scenario.estimate_setup_cost().await?;
    if min_balance < total_cost {
        return Err(ContenderError::SetupError(
            "Insufficient balance in admin account.",
            Some(format!(
                "Admin account balance: {} ETH, required: {} ETH.\nSet --min-balance to {} or higher.",
                format_ether(min_balance),
                format_ether(total_cost),
                format_ether(total_cost),
            )),
        )
        .into());
    }

    let block_time_secs = get_block_time(&rpc_client).await?;
    let timekeeper_handle = tokio::spawn(async move {
        let timeout_blocks = 10;
        let safe_time = (testconfig.create.iter().len() + timeout_blocks) as u64 * block_time_secs;
        tokio::time::sleep(Duration::from_secs(safe_time)).await;
        warn!("Contract deployment has been waiting for more than {timeout_blocks} blocks... Press Ctrl+C to cancel.");
    });
    let is_done = Arc::new(AtomicBool::new(false));

    let mut fcu_handle: Option<JoinHandle<Result<(), ContenderError>>> = None;
    if engine_params.call_fcu && scenario.auth_provider.is_some() {
        // spawn a task to advance the chain periodically while setup is running
        let auth_client = scenario.auth_provider.clone().expect("auth provider");
        let is_done = is_done.clone();
        fcu_handle = Some(tokio::task::spawn(async move {
            loop {
                if is_done.load(Ordering::SeqCst) {
                    break;
                }

                auth_client
                    .advance_chain(DEFAULT_BLOCK_TIME)
                    .await
                    .map_err(|e| ContenderError::with_err(e, "failed to advance chain"))?;
                info!("Chain advanced successfully.");

                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Ok(())
        }));
    }
    let setup_task: JoinHandle<Result<(), ContenderError>> = {
        let is_done = is_done.clone();
        tokio::task::spawn(async move {
            info!("Deploying contracts...");
            scenario.deploy_contracts().await?;
            timekeeper_handle.abort();
            info!("Finished deploying contracts. Running setup txs...");
            scenario.run_setup().await?;
            info!("Setup complete. To run the scenario, use the `spam` command.");

            // stop advancing the chain
            is_done.store(true, Ordering::SeqCst);

            Ok(())
        })
    };

    let cancel_task = {
        tokio::task::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl-c");
            warn!("CTRL-C received, stopping setup...");
        })
    };

    tokio::select! {
        task_res = setup_task => {
            task_res??;
        }

        fcu_res = async move {
            if let Some(handle) = fcu_handle {
                handle.await.map_err(|e| ContenderError::with_err(e, "failed to wait for fcu task"))??;
            } else {
                // block until ctrl-C is received
                tokio::signal::ctrl_c().await.map_err(|e| ContenderError::with_err(e, "failed to wait for ctrl-c"))?;
            }
            Ok::<_, ContenderError>(())
        } => {
            fcu_res?
        }

        _ = cancel_task => {
            warn!("Setup cancelled.");
            is_done.store(true, Ordering::SeqCst);
        },
    }

    Ok(())
}

pub struct SetupCommandArgs {
    pub scenario: SpamScenario,
    pub eth_json_rpc_args: ScenarioSendTxsCliArgs,
    pub seed: RandSeed,
}

impl SetupCommandArgs {
    pub fn new(
        scenario: SpamScenario,
        cli_args: ScenarioSendTxsCliArgs,
    ) -> contender_core::Result<Self> {
        let seed = RandSeed::seed_from_str(
            &cli_args.seed.to_owned().unwrap_or(
                load_seedfile()
                    .map_err(|e| ContenderError::with_err(e.deref(), "failed to load seedfile"))?,
            ),
        );
        Ok(Self {
            scenario,
            eth_json_rpc_args: cli_args.clone(),
            seed,
        })
    }

    async fn engine_params(&self) -> contender_core::Result<EngineParams> {
        self.eth_json_rpc_args
            .auth_args
            .engine_params(self.eth_json_rpc_args.call_forkchoice)
            .await
            .map_err(|e| ContenderError::with_err(e.deref(), "failed to build engine params"))
    }
}
