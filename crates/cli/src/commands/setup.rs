use crate::{
    util::{
        check_private_keys_fns, find_insufficient_balances, fund_accounts,
        get_signers_with_defaults, load_testconfig, EngineParams,
    },
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::utils::{format_ether, parse_ether},
    providers::{DynProvider, Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use contender_core::generator::PlanConfig;
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    error::ContenderError,
    generator::RandSeed,
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_engine_provider::DEFAULT_BLOCK_TIME;
use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use super::common::ScenarioSendTxsCliArgs;

#[derive(Debug, clap::Args)]
pub struct SetupCliArgs {
    #[command(flatten)]
    pub args: ScenarioSendTxsCliArgs,
}

pub async fn setup(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    args: SetupCommandArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let SetupCommandArgs {
        testfile,
        rpc_url,
        private_keys,
        min_balance,
        seed,
        tx_type,
        engine_params,
        env,
    } = args;

    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(url.to_owned()),
    );
    let mut testconfig = load_testconfig(&testfile).await?;

    // Setup env variables
    let mut env_variables = testconfig.env.clone().unwrap_or_default();
    if let Some(env) = env {
        for (key, value) in env {
            let _ = &env_variables.insert(key.to_string(), value.to_string());
        }
    }
    testconfig.env = Some(env_variables.clone());

    let min_balance = parse_ether(&min_balance)?;

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

        let agent = SignerStore::new(1, &seed, from_pool);
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
        tx_type,
        gas_price_percent_add: None,
        pending_tx_timeout_secs: 12,
    };

    fund_accounts(
        &all_agent_addresses,
        &admin_signer,
        &rpc_client,
        min_balance,
        tx_type,
        &engine_params,
    )
    .await?;

    let mut scenario = TestScenario::new(
        testconfig.to_owned(),
        db.clone().into(),
        seed,
        params,
        engine_params.engine_provider,
        (&PROM, &HIST),
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

    // derive block time from last two blocks. if two blocks don't exist, assume block time is 1s
    let block_num = rpc_client
        .get_block_number()
        .await
        .map_err(|e| ContenderError::with_err(e, "failed to get block number"))?;

    let block_time_secs = if block_num > 0 {
        let mut timestamps = vec![];
        for i in [0_u64, 1] {
            let block = rpc_client
                .get_block_by_number((block_num - i).into())
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get block"))?;
            if let Some(block) = block {
                timestamps.push(block.header.timestamp);
            }
        }
        if timestamps.len() == 2 {
            (timestamps[0] - timestamps[1]).max(1)
        } else {
            1
        }
    } else {
        1
    };

    let timekeeper_handle = tokio::spawn(async move {
        let timeout_blocks = 10;
        let safe_time = (testconfig.create.iter().len() + timeout_blocks) as u64 * block_time_secs;
        tokio::time::sleep(Duration::from_secs(safe_time)).await;
        warn!("Contract deployment has been waiting for more than {timeout_blocks} blocks... Press Ctrl+C to cancel.");
    });
    let is_done = Arc::new(AtomicBool::new(false));
    let (error_sender, mut error_receiver) = tokio::sync::mpsc::channel::<String>(1);
    let error_sender = Arc::new(error_sender);

    if engine_params.call_fcu && scenario.auth_provider.is_some() {
        // spawn a task to advance the chain periodically while setup is running
        let auth_client = scenario.auth_provider.clone().expect("auth provider");
        let is_done = is_done.clone();
        let error_sender = error_sender.clone();
        tokio::task::spawn(async move {
            loop {
                if is_done.load(Ordering::SeqCst) {
                    break;
                }

                let mut err = String::new();
                auth_client
                    .advance_chain(DEFAULT_BLOCK_TIME)
                    .await
                    .unwrap_or_else(|e| {
                        err = e.to_string();
                    });
                if !err.is_empty() {
                    error_sender
                        .send(err)
                        .await
                        .expect("failed to send error from task");
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }
    let setup_task: JoinHandle<Result<(), String>> = {
        let is_done = is_done.clone();
        tokio::task::spawn(async move {
            let str_err = |e| format!("Error: {e}");
            scenario.deploy_contracts().await.map_err(str_err)?;
            timekeeper_handle.abort();
            info!("Finished deploying contracts. Running setup txs...");
            scenario.run_setup().await.map_err(str_err)?;
            info!("Setup complete. To run the scenario, use the `spam` command.");

            // stop advancing the chain
            is_done.store(true, Ordering::SeqCst);

            Ok(())
        })
    };

    let cancel_task = {
        let is_done = is_done.clone();
        tokio::task::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl-c");
            warn!("CTRL-C received, stopping setup...");
            is_done.store(true, Ordering::SeqCst);
        })
    };

    tokio::select! {
        task_res = setup_task => {
            let res = task_res?;
            if let Err(e) = res {
                return Err(e.into());
            }
        }

        _ = cancel_task => {
            warn!("Setup cancelled.");
            is_done.store(true, Ordering::SeqCst);
        },

        Some(error) = error_receiver.recv() => {
            warn!(error);
            let mut help = String::new();
            if error.contains("gasLimit parameter is required") {
                help = format!("{help}You may need to set the --op flag to target this node.")
            }
            return Err(ContenderError::SetupError("Setup failed.", Some(help)).into()) // Err( format!("Setup failed. {help}").into());
        }
    }

    Ok(())
}

pub struct SetupCommandArgs {
    pub testfile: String,
    pub rpc_url: String,
    pub private_keys: Option<Vec<String>>,
    pub min_balance: String,
    pub seed: RandSeed,
    pub tx_type: TxType,
    pub engine_params: EngineParams,
    pub env: Option<Vec<(String, String)>>,
}
