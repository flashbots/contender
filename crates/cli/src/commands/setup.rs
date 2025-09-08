use crate::{
    commands::common::EngineParams,
    util::{
        check_private_keys_fns, find_insufficient_balances, fund_accounts,
        get_signers_with_defaults, load_testconfig,
    },
    LATENCY_HIST as HIST, PROM,
};
use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::{utils::format_ether, U256},
    providers::{DynProvider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    transports::http::reqwest::Url,
};
use contender_core::BundleType;
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    error::ContenderError,
    generator::RandSeed,
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_core::{generator::PlanConfig, util::get_block_time};
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
        bundle_type,
        reinit  
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
        bundle_type,
        pending_tx_timeout_secs: 12,
        extra_msg_handles: None,
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
        (&PROM, &HIST).into(),
    )
    .await?;

    // Explicit setup: force redeploy + fail on setup errors by default; allow CLI override
    let reinit_override = reinit
        .as_ref()
        .map(|s| s.eq_ignore_ascii_case("on"))
        .unwrap_or(true);
    scenario.set_always_reinit(reinit_override);
    tracing::info!(
        "setup mode: always_reinit={} ({} )",
        reinit_override,
        if reinit_override { "will redeploy and run all setup" } else { "will skip redeploy when possible" }
    );

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
    pub testfile: String,
    pub rpc_url: String,
    pub private_keys: Option<Vec<String>>,
    pub min_balance: U256,
    pub seed: RandSeed,
    pub tx_type: TxType,
    pub bundle_type: BundleType,
    pub engine_params: EngineParams,
    pub env: Option<Vec<(String, String)>>,
    pub reinit: Option<String>,
}
