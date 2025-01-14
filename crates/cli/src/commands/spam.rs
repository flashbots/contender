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
    check_private_keys, fund_accounts, get_signers_with_defaults, get_spam_pools,
    spam_callback_default, SpamCallbackType,
};

#[derive(Debug)]
pub struct SpamCommandArgs {
    pub testfile: String,
    pub rpc_url: String,
    pub builder_url: Option<String>,
    pub txs_per_block: Option<usize>,
    pub txs_per_second: Option<usize>,
    pub duration: Option<usize>,
    pub seed: String,
    pub private_keys: Option<Vec<String>>,
    pub disable_reports: bool,
    pub min_balance: String,
}

/// Runs spammer and returns run ID.
pub async fn spam(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: SpamCommandArgs,
) -> Result<u64, Box<dyn std::error::Error>> {
    let testconfig = TestConfig::from_file(&args.testfile)?;
    let rand_seed = RandSeed::seed_from_str(&args.seed);
    let url = Url::parse(&args.rpc_url).expect("Invalid RPC URL");
    let rpc_client = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_http(url.to_owned());
    let eth_client = ProviderBuilder::new().on_http(url.to_owned());

    let duration = args.duration.unwrap_or_default();
    let min_balance = parse_ether(&args.min_balance)?;

    let user_signers = get_signers_with_defaults(args.private_keys);
    let spam = testconfig
        .spam
        .as_ref()
        .expect("No spam function calls found in testfile");

    // distill all from_pool arguments from the spam requests
    let from_pool_declarations = get_spam_pools(&testconfig);

    let mut agents = AgentStore::new();
    let signers_per_period = args
        .txs_per_block
        .unwrap_or(args.txs_per_second.unwrap_or(spam.len()));

    for from_pool in &from_pool_declarations {
        if agents.has_agent(from_pool) {
            continue;
        }

        let agent = SignerStore::new_random(
            signers_per_period / from_pool_declarations.len(),
            &rand_seed,
            from_pool,
        );
        agents.add_agent(from_pool, agent);
    }

    let all_signer_addrs = [
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

    check_private_keys(&testconfig, &user_signers);

    fund_accounts(
        &all_signer_addrs,
        &user_signers[0],
        &rpc_client,
        &eth_client,
        min_balance,
    )
    .await?;

    if args.txs_per_block.is_some() && args.txs_per_second.is_some() {
        panic!("Cannot set both --txs-per-block and --txs-per-second");
    }
    if args.txs_per_block.is_none() && args.txs_per_second.is_none() {
        panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
    }

    let mut run_id = 0;

    let mut scenario = TestScenario::new(
        testconfig,
        db.clone().into(),
        url,
        args.builder_url
            .map(|url| Url::parse(&url).expect("Invalid builder URL")),
        rand_seed,
        &user_signers,
        agents,
    )
    .await?;

    if let Some(txs_per_block) = args.txs_per_block {
        println!("Blockwise spamming with {} txs per block", txs_per_block);
        let spammer = BlockwiseSpammer {};

        match spam_callback_default(!args.disable_reports, Arc::new(rpc_client).into()).await {
            SpamCallbackType::Log(cback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                run_id = db.insert_run(timestamp as u64, txs_per_block * duration)?;
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
        return Ok(run_id);
    }

    let tps = args.txs_per_second.unwrap_or(10);
    println!("Timed spamming with {} txs per second", tps);

    let interval = std::time::Duration::from_secs(1);
    let spammer = TimedSpammer::new(interval);

    match spam_callback_default(!args.disable_reports, Arc::new(rpc_client).into()).await {
        SpamCallbackType::Log(cback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            run_id = db.insert_run(timestamp as u64, tps * duration)?;
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

    Ok(run_id)
}
