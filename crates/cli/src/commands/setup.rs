use crate::util::{
    check_private_keys_fns, find_insufficient_balances, fund_accounts, get_signers_with_defaults,
    EngineParams,
};
use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::utils::{format_ether, parse_ether},
    providers::{DynProvider, ProviderBuilder},
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
use contender_testfile::TestConfig;
use std::{
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

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
    } = args;

    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .on_http(url.to_owned()),
    );
    let testconfig: TestConfig = TestConfig::from_file(testfile.as_ref())?;
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

        let agent = SignerStore::new_random(1, &seed, from_pool);
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

    let done = AtomicBool::new(false);
    let is_done = Arc::new(done);

    if engine_params.call_fcu && scenario.auth_provider.is_some() {
        let auth_client = scenario.auth_provider.clone().expect("auth provider");
        let is_done = is_done.clone();

        // spawn a task to advance the chain periodically while setup is running
        tokio::task::spawn(async move {
            loop {
                if is_done.load(Ordering::SeqCst) {
                    break;
                }

                auth_client
                    .advance_chain(DEFAULT_BLOCK_TIME)
                    .await
                    .expect("failed to advance chain");

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    scenario.deploy_contracts().await?;
    println!("Finished deploying contracts. Running setup txs...");
    scenario.run_setup().await?;
    println!("Setup complete. To run the scenario, use the `spam` command.");

    // stop advancing the chain
    is_done.store(true, Ordering::SeqCst);

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
}
