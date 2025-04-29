use super::common::{ScenarioSendTxsCliArgs, SendSpamCliArgs};
use crate::{
    util::{
        check_private_keys, fund_accounts, get_signers_with_defaults, spam_callback_default,
        EngineParams, SpamCallbackType,
    },
    LATENCY_HIST as HIST, PROM,
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
    generator::{seeder::Seeder, templater::Templater, PlanConfig, RandSeed},
    spammer::{BlockwiseSpammer, Spammer, TimedSpammer},
    test_scenario::{TestScenario, TestScenarioParams},
};
use contender_engine_provider::{
    AdvanceChain, AuthProviderEth, AuthProviderOp, DEFAULT_BLOCK_TIME,
};
use contender_testfile::TestConfig;
use std::sync::Arc;
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[derive(Debug)]
pub struct EngineArgs {
    pub auth_rpc_url: String,
    pub jwt_secret: PathBuf,
    pub use_op: bool,
}

impl EngineArgs {
    pub async fn new_provider(
        &self,
    ) -> Result<Arc<dyn AdvanceChain + Send + Sync + 'static>, Box<dyn std::error::Error>> {
        if self.use_op {
            Ok(Arc::new(AuthProviderOp::from_jwt_file(
                &self.auth_rpc_url,
                &self.jwt_secret,
            )?))
        } else {
            Ok(Arc::new(
                AuthProviderEth::from_jwt_file(&self.auth_rpc_url, &self.jwt_secret).await?,
            ))
        }
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
        long,
        long_help = "Set this to generate a report for the spam run(s) after spamming.",
        visible_aliases = &["report"]
    )]
    pub gen_report: bool,

    /// Adds (gas_price * percent) / 100 to the standard gas price of the transactions.
    #[arg(
        short,
        long,
        long_help = "Adds given percent increase to the standard gas price of the transactions."
    )]
    pub gas_price_percent_add: Option<u64>,
}

pub struct SpamCommandArgs {
    pub testfile: String,
    pub rpc_url: String,
    pub builder_url: Option<String>,
    pub txs_per_block: Option<u64>,
    pub txs_per_second: Option<u64>,
    pub duration: u64,
    pub seed: String,
    pub private_keys: Option<Vec<String>>,
    pub disable_reporting: bool,
    pub min_balance: String,
    pub tx_type: TxType,
    /// Provide to enable engine calls (required to use `call_forkchoice`)
    pub engine_params: EngineParams,
    pub gas_price_percent_add: Option<u64>,
    pub timeout_secs: u64,
}

impl SpamCommandArgs {
    pub async fn init_scenario<D: DbOps + Clone + Send + Sync + 'static>(
        &self,
        db: &D,
    ) -> Result<TestScenario<D, RandSeed, TestConfig>, Box<dyn std::error::Error>> {
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
            timeout_secs,
            engine_params,
            ..
        } = &self;

        let testconfig = TestConfig::from_file(testfile)?;
        let rand_seed = RandSeed::seed_from_str(seed);
        let url = Url::parse(rpc_url).expect("Invalid RPC URL");
        let rpc_client = DynProvider::new(
            ProviderBuilder::new()
                .network::<AnyNetwork>()
                .on_http(url.to_owned()),
        );

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
        let txs_per_duration = txs_per_block.unwrap_or(txs_per_second.unwrap_or(spam.len() as u64));
        let signers_per_period = txs_per_duration / from_pool_declarations.len().max(1) as u64;
        agents.init(
            &from_pool_declarations,
            signers_per_period as usize,
            &rand_seed,
        );

        let all_agents = agents.all_agents().collect::<Vec<_>>();
        if (txs_per_duration as usize) < all_agents.len() {
            return Err(ContenderError::SpamError(
            "Not enough signers to cover all agent pools. Set --tps or --tpb to a higher value.",
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

        let params = TestScenarioParams {
            rpc_url: url,
            builder_rpc_url: builder_url
                .to_owned()
                .map(|url| Url::parse(&url).expect("Invalid builder URL")),
            signers: user_signers.to_owned(),
            agent_store: agents.to_owned(),
            tx_type: *tx_type,
            gas_price_percent_add: *gas_price_percent_add,
            pending_tx_timeout_secs: *timeout_secs,
        };

        fund_accounts(
            &all_signer_addrs,
            &user_signers[0],
            &rpc_client,
            min_balance,
            *tx_type,
            engine_params,
        )
        .await?;

        let scenario = TestScenario::new(
            testconfig,
            db.clone().into(),
            rand_seed,
            params,
            engine_params.engine_provider.to_owned(),
            (&PROM, &HIST),
        )
        .await?;

        let total_cost = U256::from(*duration * txs_per_duration)
            * scenario.get_max_spam_cost(&user_signers).await?;
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

        Ok(scenario)
    }
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
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let SpamCommandArgs {
        txs_per_block,
        txs_per_second,
        testfile,
        duration,
        disable_reporting,
        engine_params,
        ..
    } = args;

    let mut run_id = None;

    let rpc_client = test_scenario.rpc_client.clone();
    let auth_client = test_scenario.auth_provider.to_owned();

    // thread-safe flag to stop spammer at different stages
    let done_fcu = AtomicBool::new(false);
    let is_fcu_done = Arc::new(done_fcu);
    let done_sending = AtomicBool::new(false);
    let is_sending_done = Arc::new(done_sending);

    // run loop in background to call fcu when spamming is done
    if engine_params.call_fcu {
        let is_fcu_done = is_fcu_done.clone();
        let is_sending_done = is_sending_done.clone();
        let auth_client = auth_client.clone();
        tokio::spawn(async move {
            loop {
                if is_fcu_done.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                if is_sending_done.load(std::sync::atomic::Ordering::SeqCst) {
                    if let Some(auth_client) = &auth_client {
                        let res = auth_client.advance_chain(DEFAULT_BLOCK_TIME).await;
                        if let Err(e) = res {
                            println!("Error advancing chain: {e}");
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        });
    }

    // trigger blockwise spammer
    if let Some(txs_per_block) = txs_per_block {
        println!("Blockwise spamming with {txs_per_block} txs per block");
        let spammer = BlockwiseSpammer::new();

        match spam_callback_default(
            !disable_reporting,
            engine_params.call_fcu,
            Some(rpc_client.clone()),
            auth_client,
            test_scenario.ctx.cancel_token.clone(),
        )
        .await
        {
            SpamCallbackType::Log(tx_callback) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                run_id = Some(db.insert_run(
                    timestamp as u64,
                    txs_per_block * duration,
                    testfile,
                    test_scenario.rpc_url.as_str(),
                )?);
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        *duration,
                        run_id,
                        tx_callback.into(),
                    )
                    .await?;
            }
            SpamCallbackType::Nil(tx_callback) => {
                spammer
                    .spam_rpc(
                        test_scenario,
                        *txs_per_block,
                        *duration,
                        None,
                        tx_callback.into(),
                    )
                    .await?;
            }
        };
        return Ok(run_id);
    }

    // trigger timed spammer
    let tps = txs_per_second.unwrap_or(10);
    println!("Timed spamming with {tps} txs per second");
    let spammer = TimedSpammer::new(std::time::Duration::from_secs(1));
    match spam_callback_default(
        !disable_reporting,
        engine_params.call_fcu,
        rpc_client.into(),
        auth_client,
        test_scenario.ctx.cancel_token.clone(),
    )
    .await
    {
        SpamCallbackType::Log(tx_callback) => {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();
            run_id = Some(db.insert_run(
                timestamp as u64,
                tps * duration,
                testfile,
                test_scenario.rpc_url.as_str(),
            )?);
            spammer
                .spam_rpc(test_scenario, tps, *duration, run_id, tx_callback.into())
                .await?;
        }
        SpamCallbackType::Nil(tx_callback) => {
            spammer
                .spam_rpc(test_scenario, tps, *duration, None, tx_callback.into())
                .await?;
        }
    };
    is_fcu_done.store(true, std::sync::atomic::Ordering::SeqCst);

    Ok(run_id)
}
