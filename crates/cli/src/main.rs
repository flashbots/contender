mod commands;
mod default_scenarios;
mod util;

use std::sync::LazyLock;

use alloy::hex;
use commands::{ContenderCli, ContenderSubcommand, DbCommand, SpamCommandArgs};
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
        ContenderSubcommand::Db { command } => match command {
            DbCommand::Drop => commands::drop_db().await?,
            DbCommand::Reset => commands::reset_db(&db).await?,
            DbCommand::Export { out_path } => commands::export_db(out_path).await?,
            DbCommand::Import { src_path } => commands::import_db(src_path).await?,
        },

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
            if report_file.is_some() {
                commands::report(
                    &db,
                    Some(run_id),
                    report_file.map(|rf| format!("{}.csv", rf)),
                )?;
            }
        }

        ContenderSubcommand::Report { id, out_file } => commands::report(&db, id, out_file)?,

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
