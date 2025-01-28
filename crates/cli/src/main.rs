mod commands;
mod default_scenarios;
mod util;

use std::sync::LazyLock;

use alloy::hex;
use commands::{ContenderCli, ContenderSubcommand, SpamCommandArgs};
use contender_core::{db::DbOps, generator::RandSeed};
use contender_sqlite::SqliteDb;
use rand::Rng;

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    let path = &format!(
        "{}{}",
        std::env::var("HOME").unwrap(),
        "/.contender/contender.db"
    );
    println!("opening DB at {}", path);
    std::fs::create_dir_all(std::env::var("HOME").unwrap() + "/.contender")
        .expect("failed to create ~/.contender directory");
    SqliteDb::from_file(path).expect("failed to open contender DB file")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    let _ = DB.create_tables(); // ignore error; tables already exist
    let db = DB.clone();

    let home = std::env::var("HOME").expect("$HOME not found in environment");
    let contender_path = format!("{}/.contender", home);
    if !std::path::Path::new(&contender_path).exists() {
        std::fs::create_dir_all(&contender_path).expect("failed to create contender directory");
    }

    let seed_path = format!("{}/seed", &contender_path);
    if !std::path::Path::new(&seed_path).exists() {
        println!("generating seed file at {}", &seed_path);
        let mut rng = rand::thread_rng();
        let seed: [u8; 32] = rng.gen();
        let seed_hex = hex::encode(seed);
        std::fs::write(&seed_path, seed_hex).expect("failed to write seed file");
    }

    let stored_seed = format!(
        "0x{}",
        std::fs::read_to_string(&seed_path).expect("failed to read seed file")
    );

    match args.command {
        ContenderSubcommand::Setup {
            testfile,
            rpc_url,
            private_keys,
            min_balance,
            seed,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            commands::setup(
                &db,
                testfile,
                rpc_url,
                private_keys,
                min_balance,
                RandSeed::seed_from_str(&seed),
            )
            .await?
        }

        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            builder_url,
            txs_per_block,
            txs_per_second,
            duration,
            seed,
            private_keys,
            disable_reports,
            min_balance,
            report_file,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            let run_id = commands::spam(
                &db,
                SpamCommandArgs {
                    testfile,
                    rpc_url,
                    builder_url,
                    txs_per_block,
                    txs_per_second,
                    duration,
                    seed,
                    private_keys,
                    disable_reports,
                    min_balance,
                },
            )
            .await?;
            if let Some(report_file) = report_file {
                commands::report(&db, Some(run_id), commands::ReportOutput::File(report_file))?;
            }
        }

        ContenderSubcommand::Report { id, out_file } => {
            let home_dir = std::env::var("HOME").expect("Could not get home directory");
            let contender_dir = format!("{}/.contender", home_dir);
            std::fs::create_dir_all(&contender_dir)?;
            let out_filename = out_file.unwrap_or("report".to_owned());
            let report_path = format!("{}/{}.csv", contender_dir, out_filename);
            commands::report(&db, id, commands::ReportOutput::File(report_path))?;
        }

        ContenderSubcommand::Run {
            scenario,
            rpc_url,
            private_key,
            interval,
            duration,
            txs_per_duration,
        } => {
            commands::run(
                &db,
                scenario,
                rpc_url,
                private_key,
                interval,
                duration,
                txs_per_duration,
            )
            .await?
        }
    }
    Ok(())
}
