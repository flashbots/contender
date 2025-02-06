use std::sync::Arc;

use alloy::{
    network::AnyNetwork,
    primitives::{
        utils::{format_ether, parse_ether},
        U256,
    },
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::{AgentStore, SignerStore},
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, types::AnyProvider, Generator, PlanType, RandSeed},
    spammer::{BlockwiseSpammer, ExecutionPayload, Spammer, TimedSpammer},
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

    if spam.len() == 0 {
        return Err(ContenderError::SpamError("No spam calls found in testfile", None).into());
    }

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
            signers_per_period / from_pool_declarations.len().max(1),
            &rand_seed,
            from_pool,
        );
        agents.add_agent(from_pool, agent);
    }

    let all_agents = agents.all_agents().collect::<Vec<_>>();
    let all_signer_addrs = [
        user_signers
            .iter()
            .map(|signer| signer.address())
            .collect::<Vec<_>>(),
        all_agents
            .iter()
            .flat_map(|(_, agent)| agent.signers.iter().map(|signer| signer.address()))
            .collect::<Vec<_>>(),
    ]
    .concat();

    if signers_per_period < agents.all_agents().collect::<Vec<_>>().len() {
        return Err(ContenderError::SpamError(
            "Not enough signers to cover all spam pools. Set --tps or --tpb to a higher value.",
            format!(
                "signers_per_period: {}, agents: {}",
                signers_per_period,
                all_agents.len()
            )
            .into(),
        )
        .into());
    }

    check_private_keys(&testconfig, &user_signers);

    if args.txs_per_block.is_some() && args.txs_per_second.is_some() {
        panic!("Cannot set both --txs-per-block and --txs-per-second");
    }
    if args.txs_per_block.is_none() && args.txs_per_second.is_none() {
        panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
    }

    fund_accounts(
        &all_signer_addrs,
        &user_signers[0],
        &rpc_client,
        &eth_client,
        min_balance,
    )
    .await?;

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

    let total_cost =
        get_max_spam_cost(scenario.to_owned(), &rpc_client).await? * U256::from(duration);
    if min_balance < U256::from(total_cost) {
        return Err(ContenderError::SpamError(
            "min_balance is not enough to cover the cost of the spam transactions",
            format!(
                "min_balance: {}, total_cost: {}",
                format_ether(min_balance),
                format_ether(total_cost)
            )
            .into(),
        )
        .into());
    }

    // trigger blockwise spammer
    if let Some(txs_per_block) = args.txs_per_block {
        println!("Blockwise spamming with {} txs per block", txs_per_block);
        let spammer = BlockwiseSpammer {};

        match spam_callback_default(!args.disable_reports, Arc::new(rpc_client).into()).await {
            SpamCallbackType::Log(cback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                run_id =
                    db.insert_run(timestamp as u64, txs_per_block * duration, &args.testfile)?;
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

    // trigger timed spammer
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
            run_id = db.insert_run(timestamp as u64, tps * duration, &args.testfile)?;
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

/// Returns the maximum cost of a spam transaction.
///
/// We take `scenario` by value rather than by reference, because we call `prepare_tx_request`
/// and `prepare_spam` which will mutate the scenario (namely the overly-optimistic internal nonce counter).
/// We're not going to run the transactions we generate here; we just want to see the cost of
/// our spam txs, so we can estimate how much the user should provide for `min_balance`.
async fn get_max_spam_cost<D: DbOps + Send + Sync + 'static, S: Seeder + Send + Sync>(
    scenario: TestScenario<D, S, TestConfig>,
    rpc_client: &AnyProvider,
) -> Result<U256, Box<dyn std::error::Error>> {
    let mut scenario = scenario;

    // load a sample of each spam tx from the scenario
    let sample_txs = scenario
        .prepare_spam(
            &scenario
                .load_txs(PlanType::Spam(
                    scenario
                        .config
                        .spam
                        .to_owned()
                        .map(|s| s.len()) // take the number of spam txs from the testfile
                        .unwrap_or(0),
                    |_named_req| {
                        // we can look at the named request here if needed
                        Ok(None)
                    },
                ))
                .await?,
        )
        .await?
        .iter()
        .map(|ex_payload| match ex_payload {
            ExecutionPayload::SignedTx(_envelope, tx_req) => vec![tx_req.to_owned()],
            ExecutionPayload::SignedTxBundle(_envelopes, tx_reqs) => tx_reqs.to_vec(),
        })
        .collect::<Vec<_>>()
        .concat();

    let gas_price = rpc_client.get_gas_price().await?;

    // get gas limit for each tx
    let mut prepared_sample_txs = vec![];
    for tx in sample_txs {
        let tx_req = tx.tx;
        let (prepared_req, _signer) = scenario.prepare_tx_request(&tx_req, gas_price).await?;
        println!(
            "tx_request gas={:?} gas_price={:?} ({:?}, {:?})",
            prepared_req.gas,
            prepared_req.gas_price,
            prepared_req.max_fee_per_gas,
            prepared_req.max_priority_fee_per_gas
        );
        prepared_sample_txs.push(prepared_req);
    }

    // get the highest gas cost of all spam txs
    let highest_gas_cost = prepared_sample_txs
        .iter()
        .map(|tx| {
            let mut gas_price = tx.max_fee_per_gas.unwrap_or(tx.gas_price.unwrap_or(0));
            if let Some(priority_fee) = tx.max_priority_fee_per_gas {
                gas_price += priority_fee;
            }
            println!("gas_price={:?}", gas_price);
            U256::from(gas_price * tx.gas.unwrap_or(0)) + tx.value.unwrap_or(U256::ZERO)
        })
        .max()
        .ok_or(ContenderError::SpamError(
            "failed to get max gas cost for spam txs",
            None,
        ))?;

    // we assume the highest possible cost to minimize the chances of running out of ETH mid-test
    Ok(highest_gas_cost)
}
