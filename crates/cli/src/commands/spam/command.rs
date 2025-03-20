use std::path::PathBuf;
use std::sync::Arc;

use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::{
        utils::{format_ether, parse_ether},
        U256,
    },
    providers::{DynProvider, ProviderBuilder, RootProvider},
    rpc::{client::ClientBuilder, types::engine::JwtSecret},
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::AgentStore,
    db::DbOps,
    error::ContenderError,
    eth_engine::auth_transport::AuthenticatedTransportConnect,
    generator::RandSeed,
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_testfile::TestConfig;

use crate::util::{
    check_private_keys, fund_accounts, get_signers_with_defaults, spam_callback_default,
    SpamCallbackType,
};

#[derive(Debug)]
pub struct EngineArgs {
    pub auth_rpc_url: String,
    pub jwt_secret: PathBuf,
}

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
    pub tx_type: TxType,
    /// Provide to enable engine calls (required to use `call_forkchoice`)
    pub engine_args: Option<EngineArgs>,
    /// Call `engine_forkchoiceUpdated` after each block
    pub call_forkchoice: bool,
}

/// Runs spammer and returns run ID.
pub async fn spam(
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    args: SpamCommandArgs,
) -> Result<u64, Box<dyn std::error::Error>> {
    let testconfig = TestConfig::from_file(&args.testfile)?;
    let rand_seed = RandSeed::seed_from_str(&args.seed);
    let url = Url::parse(&args.rpc_url).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .on_http(url.to_owned()),
    );
    let eth_client = DynProvider::new(ProviderBuilder::new().on_http(url.to_owned()));
    let auth_client = if let Some(engine_args) = args.engine_args {
        // parse url from engine args
        let auth_url = Url::parse(&engine_args.auth_rpc_url).expect("Invalid auth RPC URL");

        // fetch jwt from file
        //
        // the jwt is hex encoded so we will decode it after
        if !engine_args.jwt_secret.is_file() {
            return Err(ContenderError::SpamError(
                "JWT secret file not found:",
                Some(engine_args.jwt_secret.to_string_lossy().into()),
            )
            .into());
        }
        let jwt = std::fs::read_to_string(engine_args.jwt_secret)?;
        let jwt = JwtSecret::from_hex(jwt)?;

        let auth_transport = AuthenticatedTransportConnect::new(auth_url, jwt);
        let client = ClientBuilder::default()
            .connect_with(auth_transport)
            .await?;
        let auth_provider = RootProvider::<AnyNetwork>::new(client);
        Some(DynProvider::new(
            // TODO: replace this with custom auth provider
            auth_provider,
        ))
    } else {
        None
    };

    let duration = args.duration.unwrap_or_default();
    let min_balance = parse_ether(&args.min_balance)?;

    let user_signers = get_signers_with_defaults(args.private_keys);
    let spam = testconfig
        .spam
        .as_ref()
        .expect("No spam function calls found in testfile");

    if spam.is_empty() {
        return Err(ContenderError::SpamError("No spam calls found in testfile", None).into());
    }

    // distill all from_pool arguments from the spam requests
    let from_pool_declarations = testconfig.get_spam_pools();

    let mut agents = AgentStore::new();
    let signers_per_period = args
        .txs_per_block
        .unwrap_or(args.txs_per_second.unwrap_or(spam.len()));
    agents.init(
        &from_pool_declarations,
        signers_per_period / from_pool_declarations.len().max(1),
        &rand_seed,
    );

    let all_agents = agents.all_agents().collect::<Vec<_>>();
    if signers_per_period < all_agents.len() {
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

    let all_signer_addrs = agents.all_signer_addresses();

    fund_accounts(
        &all_signer_addrs,
        &user_signers[0],
        &rpc_client,
        &eth_client,
        min_balance,
        args.tx_type,
        auth_client.clone(),
    )
    .await?;

    let mut run_id = 0;

    let mut scenario = TestScenario::new(
        testconfig,
        db.clone().into(),
        rand_seed,
        TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: args
                .builder_url
                .map(|url| Url::parse(&url).expect("Invalid builder URL")),
            signers: user_signers.to_owned(),
            agent_store: agents,
            tx_type: args.tx_type,
        },
    )
    .await?;

    let total_cost = scenario.get_max_spam_cost(&user_signers).await? * U256::from(duration);
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

        match spam_callback_default(
            !args.disable_reports,
            args.call_forkchoice,
            Some(Arc::new(rpc_client)),
            auth_client.map(|c| Arc::new(c)).into(),
        )
        .await
        {
            SpamCallbackType::Log(tx_callback) => {
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
                        tx_callback.into(),
                    )
                    .await?;
            }
            SpamCallbackType::Nil(tx_callback) => {
                spammer
                    .spam_rpc(
                        &mut scenario,
                        txs_per_block,
                        duration,
                        None,
                        tx_callback.to_owned().into(),
                    )
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
    match spam_callback_default(
        !args.disable_reports,
        args.call_forkchoice,
        Arc::new(rpc_client).into(),
        auth_client.map(|c| Arc::new(c)).into(),
    )
    .await
    {
        SpamCallbackType::Log(tx_callback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            run_id = db.insert_run(timestamp as u64, tps * duration, &args.testfile)?;

            spammer
                .spam_rpc(
                    &mut scenario,
                    tps,
                    duration,
                    Some(run_id),
                    tx_callback.into(),
                )
                .await?;
        }
        SpamCallbackType::Nil(cback) => {
            spammer
                .spam_rpc(&mut scenario, tps, duration, None, cback.to_owned().into())
                .await?;
        }
    };

    Ok(run_id)
}
