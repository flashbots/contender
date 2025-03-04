use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::utils::{format_ether, parse_ether},
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
use contender_testfile::TestConfig;
use std::str::FromStr;

use crate::util::{
    check_private_keys_fns, find_insufficient_balances, fund_accounts, get_create_pools,
    get_setup_pools, get_signers_with_defaults,
};

pub async fn setup(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    testfile: impl AsRef<str>,
    rpc_url: impl AsRef<str>,
    private_keys: Option<Vec<String>>,
    min_balance: String,
    seed: RandSeed,
    tx_type: TxType,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .on_http(url.to_owned()),
    );
    let eth_client = DynProvider::new(ProviderBuilder::new().on_http(url.to_owned()));
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
        [get_setup_pools(&testconfig), get_create_pools(&testconfig)].concat();

    // create agents for each from_pool declaration
    let mut agents = AgentStore::new();
    for from_pool in &from_pool_declarations {
        if agents.has_agent(from_pool) {
            continue;
        }

        let agent = SignerStore::new_random(1, &seed, from_pool);
        agents.add_agent(from_pool, agent);
    }

    let all_signer_addrs = [
        // don't include default accounts (`user_signers_with_defaults`) here because if you're using them, they should already be funded
        user_signers
            .iter()
            .map(|signer| signer.address())
            .collect::<Vec<_>>(),
        agents
            .all_agents()
            .flat_map(|(_, agent)| agent.signers.iter().map(|signer| signer.address()))
            .collect::<Vec<_>>(),
    ]
    .concat();

    let admin_signer = &user_signers_with_defaults[0];

    fund_accounts(
        &all_signer_addrs,
        admin_signer,
        &rpc_client,
        &eth_client,
        min_balance,
        tx_type,
    )
    .await?;

    let mut scenario = TestScenario::new(
        testconfig.to_owned(),
        db.clone().into(),
        seed,
        TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: None,
            signers: user_signers_with_defaults,
            agent_store: agents,
            tx_type,
        },
    )
    .await?;

    scenario.deploy_contracts().await?;
    println!("Finished deploying contracts. Running setup txs...");
    scenario.run_setup().await?;
    println!("Setup complete. To run the scenario, use the `spam` command.");

    Ok(())
}
