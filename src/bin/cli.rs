mod cli_lib;

use cli_lib::{ContenderCli, ContenderSubcommand};
use contender_core::{
    db::{database::DbOps, sqlite::SqliteDb},
    generator::{
        testfile::{SetupGenerator, SpamGenerator, TestConfig},
        RandSeed,
    },
    spammer::Spammer,
};
use std::sync::LazyLock;

// TODO: is this the best solution? feels like there's something better out there lmao
static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    SqliteDb::from_file("contender.db").expect("failed to open contender.db")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    let _ = DB.create_tables(); // ignore error; tables already exist
    match args.command {
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            intensity,
            duration,
            seed,
        } => {
            let testfile = TestConfig::from_file(&testfile)?;
            let rand_seed = seed.map(|s| RandSeed::from_str(&s)).unwrap_or_default();
            let gen = SpamGenerator::new(testfile, &rand_seed);
            let spammer = Spammer::new(gen, &*DB, rpc_url);
            spammer.spam_rpc(intensity.unwrap_or_default(), duration.unwrap_or_default())?;
        }
        ContenderSubcommand::Setup { testfile, rpc_url } => {
            let gen: SetupGenerator = TestConfig::from_file(&testfile)?.into();
            let spammer = Spammer::new(gen, &*DB, rpc_url);
            spammer.spam_rpc(10, 1)?;
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
