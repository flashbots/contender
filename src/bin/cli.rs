mod cli_lib;

use alloy::{
    providers::ProviderBuilder, signers::local::PrivateKeySigner, transports::http::reqwest::Url,
};
use cli_lib::{ContenderCli, ContenderSubcommand};
use contender_core::{
    db::{database::DbOps, sqlite::SqliteDb},
    generator::{
        testfile::{LogCallback, NilCallback},
        types::{RpcProvider, TestConfig},
        RandSeed,
    },
    spammer::{BlockwiseSpammer, TimedSpammer},
    test_scenario::TestScenario,
};
use std::{
    str::FromStr,
    sync::{Arc, LazyLock},
};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    SqliteDb::from_file("contender.db").expect("failed to open contender.db")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    let _ = DB.create_tables(); // ignore error; tables already exist
    match args.command {
        ContenderSubcommand::Setup {
            testfile,
            rpc_url,
            private_keys,
        } => {
            let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
            let testconfig: TestConfig = TestConfig::from_file(&testfile)?;

            let private_keys = private_keys.expect("Must provide private keys for setup");
            let signers: Vec<PrivateKeySigner> = private_keys
                .iter()
                .map(|k| PrivateKeySigner::from_str(k).expect("Invalid private key"))
                .collect();

            let scenario = TestScenario::new(
                testconfig.to_owned(),
                Arc::new(DB.clone()),
                url,
                RandSeed::new(),
                &signers,
            );

            scenario.deploy_contracts().await?;
            scenario.run_setup().await?;
            // TODO: catch failures and prompt user to retry specific steps
        }
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            txs_per_block,
            txs_per_second,
            duration,
            seed,
            private_keys,
            disable_reports,
        } => {
            let testfile = TestConfig::from_file(&testfile)?;
            let rand_seed = seed.map(|s| RandSeed::from_str(&s)).unwrap_or_default();
            let url = Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL");
            let rpc_client = Arc::new(ProviderBuilder::new().on_http(url.to_owned()));
            let duration = duration.unwrap_or_default();

            let signers = private_keys.as_ref().map(|keys| {
                keys.iter()
                    .map(|k| PrivateKeySigner::from_str(k).expect("Invalid private key"))
                    .collect::<Vec<PrivateKeySigner>>()
            });

            if txs_per_block.is_some() && txs_per_second.is_some() {
                panic!("Cannot set both --txs-per-block and --txs-per-second");
            }

            if let Some(txs_per_block) = txs_per_block {
                let signers = signers.expect("must provide private keys for blockwise spamming");
                let scenario =
                    TestScenario::new(testfile, DB.clone().into(), url, rand_seed, &signers);
                println!("Blockwise spamming with {} txs per block", txs_per_block);
                match spam_callback_default(!disable_reports, rpc_client.into()).await {
                    SpamCallbackType::Log(cback) => {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .expect("Time went backwards")
                            .as_millis();
                        let run_id = cback
                            .db
                            .clone()
                            .insert_run(timestamp as u64, txs_per_block * duration)?;
                        let spammer = BlockwiseSpammer::new(scenario, cback, rpc_url);
                        spammer
                            .spam_rpc(txs_per_block, duration, Some(run_id.into()))
                            .await?;
                        println!("Saved run. run_id = {}", run_id);
                    }
                    SpamCallbackType::Nil(cback) => {
                        let spammer = BlockwiseSpammer::new(scenario, cback, rpc_url);
                        spammer.spam_rpc(txs_per_block, duration, None).await?;
                    }
                };
                return Ok(());
            }

            // private keys are not used for timed spamming; timed spamming only works with unlocked accounts
            let scenario = TestScenario::new(testfile, DB.clone().into(), url, rand_seed, &[]);
            let tps = txs_per_second.unwrap_or(10);
            println!("Timed spamming with {} txs per second", tps);
            let spammer = TimedSpammer::new(scenario, NilCallback::new(), rpc_url);
            spammer.spam_rpc(tps, duration).await?;
        }
        ContenderSubcommand::Report { id, out_file } => {
            println!(
                "Exporting report for run ID {:?} to out_file {:?}",
                id, out_file
            );
            todo!();
        }
    }
    Ok(())
}

enum SpamCallbackType<D: DbOps + Send + Sync> {
    Log(LogCallback<D>),
    Nil(NilCallback),
}

async fn spam_callback_default(
    log_txs: bool,
    rpc_client: Option<Arc<RpcProvider>>,
) -> SpamCallbackType<SqliteDb> {
    if log_txs {
        SpamCallbackType::Log(LogCallback::new(
            Arc::new(DB.clone()),
            rpc_client.unwrap().clone(),
        ))
    } else {
        SpamCallbackType::Nil(NilCallback::new())
    }
}
