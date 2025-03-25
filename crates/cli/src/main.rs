mod commands;
mod default_scenarios;
mod util;

use std::sync::LazyLock;

use alloy::hex;
use commands::{ContenderCli, ContenderSubcommand, DbCommand, RunCommandArgs, SpamCommandArgs};
use contender_core::{db::DbOps, generator::RandSeed};
use contender_sqlite::{SqliteDb, DB_VERSION};
use rand::Rng;
use util::{data_dir, db_file};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    let path = db_file().expect("failed to get DB file path");
    println!("opening DB at {}", path);
    SqliteDb::from_file(&path).expect("failed to open contender DB file")
});

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse_args();
    if DB.table_exists("run_txs")? {
        // check version and exit if DB version is incompatible
        let quit_early = DB.version() < DB_VERSION
            && !matches!(&args.command, ContenderSubcommand::Db { command: _ });
        if quit_early {
            println!("Your database is incompatible with this version of contender. To backup your data, run `contender db export`.\nPlease run `contender db drop` before trying again.");
            return Ok(());
        }
    } else {
        println!("no DB found, creating new DB");
        DB.create_tables()?;
    }
    let db = DB.clone();
    let data_path = data_dir()?;
    let db_path = db_file()?;

    let seed_path = format!("{}/seed", &data_path);
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
        ContenderSubcommand::Db { command } => match command {
            DbCommand::Drop => commands::drop_db(&db_path).await?,
            DbCommand::Reset => commands::reset_db(&db_path).await?,
            DbCommand::Export { out_path } => commands::export_db(&db_path, out_path).await?,
            DbCommand::Import { src_path } => commands::import_db(src_path, &db_path).await?,
        },

        ContenderSubcommand::Setup {
            testfile,
            rpc_url,
            private_keys,
            min_balance,
            seed,
            tx_type,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            commands::setup(
                &db,
                testfile,
                rpc_url,
                private_keys,
                min_balance,
                RandSeed::seed_from_str(&seed),
                tx_type.into(),
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
            gen_report,
            tx_type,
            gas_price_percent_add,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            let run_id = commands::spam(
                &db,
                SpamCommandArgs {
                    testfile,
                    rpc_url: rpc_url.to_owned(),
                    builder_url,
                    txs_per_block,
                    txs_per_second,
                    duration,
                    seed,
                    private_keys,
                    disable_reports,
                    min_balance,
                    tx_type: tx_type.into(),
                    gas_price_percent_add,
                },
            )
            .await?;
            if gen_report {
                commands::report(Some(run_id), 0, &db, &rpc_url).await?;
            }
        }

        ContenderSubcommand::Report {
            rpc_url,
            last_run_id,
            preceding_runs,
        } => {
            commands::report(last_run_id, preceding_runs, &db, &rpc_url).await?;
        }

        ContenderSubcommand::Run {
            scenario,
            rpc_url,
            private_key,
            interval,
            duration,
            txs_per_duration,
            skip_deploy_prompt,
            tx_type,
        } => {
            commands::run(
                &db,
                RunCommandArgs {
                    scenario,
                    rpc_url,
                    private_key,
                    interval,
                    duration,
                    txs_per_duration,
                    skip_deploy_prompt,
                    tx_type: tx_type.into(),
                },
            )
            .await?
        }
    }
    Ok(())
}
