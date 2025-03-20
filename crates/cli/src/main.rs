mod commands;
mod default_scenarios;
mod util;

use std::sync::LazyLock;

use alloy::hex;
use commands::{
    db::{drop_db, export_db, import_db, reset_db},
    run::RunCommandArgs,
    setup::SetupCommandArgs,
    spam::{EngineArgs, SpamCommandArgs},
    ContenderCli, ContenderSubcommand, DbCommand,
};
use contender_core::{db::DbOps, generator::RandSeed};
use contender_sqlite::SqliteDb;
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
    DB.create_tables()?;
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
            DbCommand::Drop => drop_db(&db_path).await?,
            DbCommand::Reset => reset_db(&db_path).await?,
            DbCommand::Export { out_path } => export_db(&db_path, out_path).await?,
            DbCommand::Import { src_path } => import_db(src_path, &db_path).await?,
        },

        ContenderSubcommand::Setup {
            testfile,
            rpc_url,
            private_keys,
            min_balance,
            seed,
            tx_type,
            auth_rpc_url,
            jwt_secret,
            call_forkchoice,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            if call_forkchoice && (auth_rpc_url.is_none() || jwt_secret.is_none()) {
                return Err("auth-rpc-url and jwt-secret required for forkchoice".into());
            }
            let engine_args = if auth_rpc_url.is_some() && jwt_secret.is_some() {
                Some(EngineArgs {
                    auth_rpc_url: auth_rpc_url.expect("auth_rpc_url"),
                    jwt_secret: jwt_secret.expect("jwt_secret").into(),
                })
            } else {
                None
            };
            commands::setup(
                &db,
                SetupCommandArgs {
                    testfile,
                    rpc_url,
                    private_keys,
                    min_balance,
                    seed: RandSeed::seed_from_str(&seed),
                    tx_type: tx_type.into(),
                    engine_args,
                    call_fcu: call_forkchoice,
                },
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
            auth_rpc_url,
            jwt_secret,
            call_forkchoice,
        } => {
            let seed = seed.unwrap_or(stored_seed);
            let engine_args = if auth_rpc_url.is_some() && jwt_secret.is_some() {
                Some(EngineArgs {
                    auth_rpc_url: auth_rpc_url.expect("auth_rpc_url"),
                    jwt_secret: jwt_secret.expect("jwt_secret").into(),
                })
            } else {
                None
            };
            if call_forkchoice && engine_args.is_none() {
                return Err("engine args required for forkchoice".into());
            }
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
                    engine_args,
                    call_forkchoice,
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
            auth_rpc_url,
            jwt_secret,
            call_forkchoice,
        } => {
            if call_forkchoice && (auth_rpc_url.is_none() || jwt_secret.is_none()) {
                return Err("auth-rpc-url and jwt-secret required for forkchoice".into());
            }
            let engine_args = if auth_rpc_url.is_some() && jwt_secret.is_some() {
                Some(EngineArgs {
                    auth_rpc_url: auth_rpc_url.expect("auth_rpc_url"),
                    jwt_secret: jwt_secret.expect("jwt_secret").into(),
                })
            } else {
                None
            };
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
                    engine_args,
                    call_fcu: call_forkchoice,
                },
            )
            .await?
        }
    }
    Ok(())
}
