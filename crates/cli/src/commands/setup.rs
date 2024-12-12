use alloy::{
    network::AnyNetwork, primitives::utils::parse_ether, providers::ProviderBuilder,
    signers::local::PrivateKeySigner, transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    generator::RandSeed,
    test_scenario::TestScenario,
};
use contender_testfile::TestConfig;
use std::str::FromStr;

use crate::util::{
    check_private_keys_fns, find_insufficient_balance_addrs, fund_accounts, get_setup_pools,
    get_signers_with_defaults,
};

pub async fn setup(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    testfile: impl AsRef<str>,
    rpc_url: impl AsRef<str>,
    private_keys: Option<Vec<String>>,
    min_balance: String,
    seed: RandSeed,
    signers_per_period: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_http(url.to_owned());
    let eth_client = ProviderBuilder::new().on_http(url.to_owned());
    let testconfig: TestConfig = TestConfig::from_file(testfile.as_ref())?;
    let min_balance = parse_ether(&min_balance)?;

    let user_signers = private_keys
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|key| PrivateKeySigner::from_str(key).expect("invalid private key"))
        .collect::<Vec<PrivateKeySigner>>();
    let default_signers = get_signers_with_defaults(private_keys);
    check_private_keys_fns(
        &testconfig.setup.to_owned().unwrap_or_default(),
        &default_signers,
    );
    let broke_accounts = find_insufficient_balance_addrs(
        &user_signers.iter().map(|s| s.address()).collect::<Vec<_>>(),
        min_balance,
        &rpc_client,
    )
    .await?;
    if !broke_accounts.is_empty() {
        panic!("Insufficient balance in provided user account(s)");
    }

    let mut agents = AgentStore::new();
    let from_pools = get_setup_pools(&testconfig);
    let mut all_signers = vec![];
    all_signers.extend_from_slice(&user_signers);

    for from_pool in &from_pools {
        if agents.has_agent(from_pool) {
            continue;
        }

        let agent = SignerStore::new_random(signers_per_period, &seed, from_pool);
        all_signers.extend_from_slice(&agent.signers);
        agents.add_agent(from_pool, agent);
    }

    println!("all signers: {:?}", all_signers);
    println!("default_signers[0]: {:?}", default_signers[0]);

    fund_accounts(
        &rpc_client,
        &eth_client,
        min_balance,
        &all_signers,
        &default_signers[0],
    )
    .await?;

    println!("****** funded accounts");

    let mut scenario = TestScenario::new(
        testconfig.to_owned(),
        db.clone().into(),
        url,
        None,
        RandSeed::new(),
        &default_signers,
        agents,
    )
    .await?;

    scenario.deploy_contracts().await?;
    scenario.run_setup().await?;
    // TODO: catch failures and prompt user to retry specific steps

    Ok(())
}
