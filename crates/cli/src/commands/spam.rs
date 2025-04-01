use super::common::{ScenarioSendTxsCliArgs, SendSpamCliArgs};
use crate::util::{
    check_private_keys, fund_accounts, get_signers_with_defaults, spam_callback_default,
    SpamCallbackType,
};
use alloy::{
    consensus::TxType,
    network::AnyNetwork,
    primitives::{
        utils::{format_ether, parse_ether},
        U256,
    },
    providers::{DynProvider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use contender_core::{
    agent_controller::AgentStore,
    db::DbOps,
    error::ContenderError,
    eth_engine::{advance_chain, get_auth_provider, DEFAULT_BLOCK_TIME},
    generator::{seeder::Seeder, templater::Templater, types::AnyProvider, PlanConfig, RandSeed},
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;
use std::sync::Arc;
use std::{path::PathBuf, sync::atomic::AtomicBool};

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
    pub disable_reporting: bool,
    pub min_balance: String,
    pub tx_type: TxType,
    pub gas_price_percent_add: Option<u16>,
    /// Provide to enable engine calls (required to use `call_forkchoice`)
    pub engine_args: Option<EngineArgs>,
    /// Call `engine_forkchoiceUpdated` after each block
    pub call_forkchoice: bool,
}

impl SpamCommandArgs {
    pub async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
        &self,
        db: &D,
    ) -> Result<InitializedScenario<D>, Box<dyn std::error::Error>> {
        init_scenario(db, self).await
    }
}

#[derive(Debug, clap::Args)]
pub struct SpamCliArgs {
    #[command(flatten)]
    pub eth_json_rpc_args: ScenarioSendTxsCliArgs,

    #[command(flatten)]
    pub spam_args: SendSpamCliArgs,

    /// Whether to log reports for the spamming run.
    #[arg(
            long,
            long_help = "Prevent tx results from being saved to DB.",
            visible_aliases = &["dr"]
        )]
    pub disable_reporting: bool,

    /// The path to save the report to.
    /// If not provided, the report can be generated with the `report` subcommand.
    /// If provided, the report is saved to the given path.
    #[arg(
        short = 'r',
        long,
        long_help = "Filename of the saved report. May be a fully-qualified path. If not provided, the report can be generated with the `report` subcommand. '.csv' extension is added automatically."
    )]
    pub gen_report: bool,

    /// Adds (gas_price * percent) / 100 to the standard gas price of the transactions.
    #[arg(
        short,
        long,
        long_help = "Adds given percent increase to the standard gas price of the transactions."
    )]
    pub gas_price_percent_add: Option<u16>,
}

pub struct InitializedScenario<D = SqliteDb, S = RandSeed, P = TestConfig>
where
    D: DbOps + Clone + Send + Sync + 'static,
    S: Seeder,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    pub scenario: TestScenario<D, S, P>,
    pub rpc_client: AnyProvider,
}

/// Initializes a TestScenario with the given arguments.
async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
    db: &D,
    args: &SpamCommandArgs,
) -> Result<InitializedScenario<D>, Box<dyn std::error::Error>> {
    println!("Initializing spammer...");
    let SpamCommandArgs {
        txs_per_block,
        txs_per_second,
        testfile,
        duration,
        seed,
        rpc_url,
        builder_url,
        min_balance,
        private_keys,
        tx_type,
        gas_price_percent_add,
        call_forkchoice,
        engine_args,
        ..
    } = &args;

    let testconfig = TestConfig::from_file(testfile)?;
    let rand_seed = RandSeed::seed_from_str(seed);
    let url = Url::parse(rpc_url).expect("Invalid RPC URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .on_http(url.to_owned()),
    );
    let auth_client = if let Some(engine_args) = engine_args {
        Some(get_auth_provider(&engine_args.auth_rpc_url, engine_args.jwt_secret.to_owned()).await?)
    } else {
        None
    };

    let duration = duration.unwrap_or_default();
    let min_balance = parse_ether(min_balance)?;

    let user_signers = get_signers_with_defaults(private_keys.to_owned());
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
    let signers_per_period = txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam.len()));
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

    if txs_per_block.is_some() && txs_per_second.is_some() {
        panic!("Cannot set both --txs-per-block and --txs-per-second");
    }
    if txs_per_block.is_none() && txs_per_second.is_none() {
        panic!("Must set either --txs-per-block (--tpb) or --txs-per-second (--tps)");
    }

    let all_signer_addrs = agents.all_signer_addresses();

    fund_accounts(
        &all_signer_addrs,
        &user_signers[0],
        &rpc_client,
        min_balance,
        *tx_type,
        (auth_client.clone(), *call_forkchoice),
    )
    .await?;

    let scenario = TestScenario::new(
        testconfig,
        db.clone().into(),
        rand_seed,
        TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: builder_url
                .to_owned()
                .map(|url| Url::parse(&url).expect("Invalid builder URL")),
            signers: user_signers.to_owned(),
            agent_store: agents.to_owned(),
            tx_type: *tx_type,
            gas_price_percent_add: *gas_price_percent_add,
        },
    )
    .await?;

    // don't multiply by TPS or TPB, because that number scales the number of accounts; this cost is per account
    let total_cost = U256::from(duration) * scenario.get_max_spam_cost(&user_signers).await?;
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

    Ok(InitializedScenario {
        scenario,
        rpc_client,
    })
}

/// Runs spammer and returns run ID.
pub async fn spam<
    D: DbOps + Clone + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
>(
    db: &D,
    args: &SpamCommandArgs,
    test_scenario: &mut TestScenario<D, S, P>,
    rpc_client: &AnyProvider,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let SpamCommandArgs {
        txs_per_block,
        txs_per_second,
        testfile,
        duration,
        disable_reporting,
        call_forkchoice,
        engine_args,
        ..
    } = args;

    let duration = duration.unwrap_or_default();
    println!("Duration: {} seconds", duration);
    let mut run_id = None;
    let rpc_client = Arc::new(rpc_client.to_owned());

    let auth_client = if let Some(engine_args) = engine_args {
        Some(get_auth_provider(&engine_args.auth_rpc_url, engine_args.jwt_secret.to_owned()).await?)
    } else {
        None
    };

    // thread-safe flag to stop spammer at different stages
    let done_fcu = AtomicBool::new(false);
    let is_fcu_done = Arc::new(done_fcu);
    let done_sending = AtomicBool::new(false);
    let is_sending_done = Arc::new(done_sending);

    // run loop in background to call fcu when spamming is done
    if let Some(auth_client) = auth_client.to_owned() {
        let auth_client = Arc::new(auth_client);
        let is_fcu_done = is_fcu_done.clone();
        let is_sending_done = is_sending_done.clone();
        tokio::spawn(async move {
            loop {
                if is_fcu_done.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                if is_sending_done.load(std::sync::atomic::Ordering::SeqCst) {
                    let res = advance_chain(&auth_client, DEFAULT_BLOCK_TIME).await;
                    if let Err(e) = res {
                        println!("Error advancing chain: {}", e);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        });
    }

    // trigger blockwise spammer
    if let Some(txs_per_block) = txs_per_block {
        println!("Blockwise spamming with {} txs per block", txs_per_block);
        let spammer = BlockwiseSpammer {};

        match spam_callback_default(
            !disable_reporting,
            *call_forkchoice,
            Some(rpc_client.clone()),
            auth_client.map(Arc::new),
        )
        .await
        {
            SpamCallbackType::Log(tx_callback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                run_id =
                    Some(db.insert_run(timestamp as u64, txs_per_block * duration, testfile)?);
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        duration,
                        run_id,
                        tx_callback.into(),
                        is_sending_done.clone(),
                    )
                    .await?;
            }
            SpamCallbackType::Nil(tx_callback) => {
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        duration,
                        None,
                        tx_callback.to_owned().into(),
                        is_sending_done.clone(),
                    )
                    .await?;
            }
        };
        return Ok(run_id);
    }

    // trigger timed spammer
    let tps = txs_per_second.unwrap_or(10);
    println!("Timed spamming with {} txs per second", tps);
    let interval = std::time::Duration::from_secs(1);
    let spammer = TimedSpammer::new(interval);
    match spam_callback_default(
        !disable_reporting,
        *call_forkchoice,
        rpc_client.into(),
        auth_client.map(Arc::new),
    )
    .await
    {
        SpamCallbackType::Log(tx_callback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            run_id = Some(db.insert_run(timestamp as u64, tps * duration, testfile)?);

            spammer
                .spam_rpc(
                    test_scenario,
                    tps,
                    duration,
                    run_id,
                    tx_callback.into(),
                    is_sending_done.clone(),
                )
                .await?;
        }
        SpamCallbackType::Nil(cback) => {
            spammer
                .spam_rpc(
                    test_scenario,
                    tps,
                    duration,
                    None,
                    cback.to_owned().into(),
                    is_sending_done.clone(),
                )
                .await?;
        }
    };
    is_fcu_done.store(true, std::sync::atomic::Ordering::SeqCst);

    Ok(run_id)
}
