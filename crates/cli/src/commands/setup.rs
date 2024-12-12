use alloy::{
    network::AnyNetwork, primitives::utils::parse_ether, providers::ProviderBuilder,
    signers::local::PrivateKeySigner, transports::http::reqwest::Url,
};
use contender_core::{generator::RandSeed, test_scenario::TestScenario};
use contender_testfile::TestConfig;
use std::str::FromStr;

use crate::util::{
    check_private_keys_fns, find_insufficient_balance_addrs, get_signers_with_defaults,
};

pub async fn setup(
    db: &(impl contender_core::db::DbOps + Clone + Send + Sync + 'static),
    testfile: impl AsRef<str>,
    rpc_url: impl AsRef<str>,
    private_keys: Option<Vec<String>>,
    min_balance: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_http(url.to_owned());
    let testconfig: TestConfig = TestConfig::from_file(testfile.as_ref())?;
    let min_balance = parse_ether(&min_balance)?;

    let user_signers = private_keys
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|key| PrivateKeySigner::from_str(key).expect("invalid private key"))
        .collect::<Vec<PrivateKeySigner>>();
    let signers = get_signers_with_defaults(private_keys);
    check_private_keys_fns(
        &testconfig.setup.to_owned().unwrap_or_default(),
        signers.as_slice(),
    );
    let broke_accounts = find_insufficient_balance_addrs(
        &user_signers.iter().map(|s| s.address()).collect::<Vec<_>>(),
        min_balance,
        &rpc_client,
    )
    .await?;
    if !broke_accounts.is_empty() {
        panic!("Some accounts do not have sufficient balance");
    }

    let mut scenario = TestScenario::new(
        testconfig.to_owned(),
        db.clone().into(),
        url,
        None,
        RandSeed::new(),
        &signers,
        Default::default(),
    )
    .await?;

    scenario.deploy_contracts().await?;
    scenario.run_setup().await?;
    // TODO: catch failures and prompt user to retry specific steps

    Ok(())
}
