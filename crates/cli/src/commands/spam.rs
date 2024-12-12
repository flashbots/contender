use std::sync::Arc;

use alloy::{
    network::AnyNetwork, primitives::utils::parse_ether, providers::ProviderBuilder,
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    db::DbOps,
    generator::RandSeed,
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::TestScenario,
};
use contender_testfile::TestConfig;

use crate::util::{
    check_private_keys, fund_accounts, get_from_pools, get_signers_with_defaults,
    spam_callback_default, SpamCallbackType,
};

pub async fn spam(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    testfile: impl AsRef<str>,
    rpc_url: impl AsRef<str>,
    builder_url: Option<String>,
    txs_per_block: Option<usize>,
    txs_per_second: Option<usize>,
    duration: Option<usize>,
    seed: Option<String>,
    private_keys: Option<Vec<String>>,
    disable_reports: bool,
    min_balance: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let testconfig = TestConfig::from_file(testfile.as_ref())?;
    let rand_seed = seed
        .map(|s| RandSeed::seed_from_str(s.as_ref()))
        .unwrap_or_default();
    let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
    let rpc_client = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_http(url.to_owned());
    let eth_client = ProviderBuilder::new().on_http(url.to_owned());

    let duration = duration.unwrap_or_default();
    let min_balance = parse_ether(min_balance.as_ref())?;

    let user_signers = get_signers_with_defaults(private_keys);
    let spam = testconfig
        .spam
        .as_ref()
        .expect("No spam function calls found in testfile");

    // distill all from_pool arguments from the spam requests
    let from_pools = get_from_pools(&testconfig);

    let mut agents = AgentStore::new();
    let signers_per_period =
        txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam.len())) / spam.len();

    let mut all_signers = vec![];
    all_signers.extend_from_slice(&user_signers);

    for from_pool in &from_pools {
        if agents.has_agent(from_pool) {
            continue;
        }

        let agent = SignerStore::new_random(signers_per_period, &rand_seed, from_pool);
        all_signers.extend_from_slice(&agent.signers);
        agents.add_agent(from_pool, agent);
    }

    check_private_keys(&testconfig, &all_signers);

    fund_accounts(
        &rpc_client,
        &eth_client,
        min_balance,
        &all_signers,
        &user_signers[0],
    )
    .await?;

    if txs_per_block.is_some() && txs_per_second.is_some() {
        panic!("Cannot set both --txs-per-block and --txs-per-second");
    }
    if txs_per_block.is_none() && txs_per_second.is_none() {
        panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
    }

    let mut scenario = TestScenario::new(
        testconfig,
        db.clone().into(),
        url,
        builder_url.map(|url| Url::parse(&url).expect("Invalid builder URL")),
        rand_seed,
        &user_signers,
        agents,
    )
    .await?;

    if let Some(txs_per_block) = txs_per_block {
        println!("Blockwise spamming with {} txs per block", txs_per_block);
        let spammer = BlockwiseSpammer {};

        match spam_callback_default(!disable_reports, Arc::new(rpc_client).into()).await {
            SpamCallbackType::Log(cback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                let run_id = db.insert_run(timestamp as u64, txs_per_block * duration)?;
                spammer
                    .spam_rpc(
                        &mut scenario,
                        txs_per_block,
                        duration,
                        Some(run_id),
                        cback.into(),
                    )
                    .await?;
            }
            SpamCallbackType::Nil(cback) => {
                spammer
                    .spam_rpc(&mut scenario, txs_per_block, duration, None, cback.into())
                    .await?;
            }
        };
        return Ok(());
    }

    let tps = txs_per_second.unwrap_or(10);
    println!("Timed spamming with {} txs per second", tps);

    let interval = std::time::Duration::from_secs(1);
    let spammer = TimedSpammer::new(interval);

    match spam_callback_default(!disable_reports, Arc::new(rpc_client).into()).await {
        SpamCallbackType::Log(cback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            let run_id = db.insert_run(timestamp as u64, tps * duration)?;
            spammer
                .spam_rpc(&mut scenario, tps, duration, Some(run_id), cback.into())
                .await?;
        }
        SpamCallbackType::Nil(cback) => {
            spammer
                .spam_rpc(&mut scenario, tps, duration, None, cback.into())
                .await?;
        }
    };

    Ok(())
}
