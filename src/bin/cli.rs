mod cli_lib;

use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use cli_lib::{ContenderCli, ContenderSubcommand};
use contender_core::{
    db::{database::DbOps, sqlite::SqliteDb},
    generator::{
        testfile::{
            ContractDeployer, NilCallback, SetupCallback, SetupGenerator, SpamGenerator, TestConfig,
        },
        RandSeed,
    },
    spammer::timed::TimedSpammer,
};
use std::sync::{Arc, LazyLock};

// TODO: is this the best solution? feels like there's something better out there lmao
static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    SqliteDb::from_file("contender.db").expect("failed to open contender.db")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    let _ = DB.create_tables(); // ignore error; tables already exist
    match args.command {
        ContenderSubcommand::Setup { testfile, rpc_url } => {
            let rpc_client = Arc::new(
                ProviderBuilder::new()
                    .on_http(Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL")),
            );
            let testconfig: TestConfig = TestConfig::from_file(&testfile)?;

            // process [[create]] steps; deploys contracts one at a time, updating the DB with each address
            // so that subsequent steps can reference them
            let deployer = ContractDeployer::<SqliteDb>::new(
                testconfig.to_owned(),
                Arc::new(DB.clone()),
                rpc_client.clone(),
            );
            deployer.run().await?;

            // process [[setup]] steps; generates transactions and sends them to the RPC via a Spammer
            let gen = SetupGenerator::<SqliteDb>::new(testconfig, DB.clone());
            let callback = SetupCallback::new(Arc::new(DB.clone()), rpc_client.clone());
            let spammer = TimedSpammer::new(gen, callback, rpc_url);
            spammer.spam_rpc(10, 1).await?;
        }
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            intensity,
            duration,
            seed,
        } => {
            let testfile = TestConfig::from_file(&testfile)?;
            let rand_seed = seed.map(|s| RandSeed::from_str(&s)).unwrap_or_default();
            let gen = SpamGenerator::new(testfile, &rand_seed, DB.clone());
            let callback = NilCallback::new();
            let spammer = TimedSpammer::new(gen, callback, rpc_url);
            spammer
                .spam_rpc(intensity.unwrap_or_default(), duration.unwrap_or_default())
                .await?;
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
