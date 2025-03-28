mod commands;
mod default_scenarios;
mod util;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use alloy::hex;
use commands::{
    common::{ScenarioSendTxsCliArgs, SendSpamCliArgs},
    ContenderCli, ContenderSubcommand, DbCommand, RunCommandArgs, SetupCliArgs, SpamCliArgs,
    SpamCommandArgs,
};
use contender_core::{db::DbOps, error::ContenderError, generator::RandSeed};
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
            args:
                SetupCliArgs {
                    args:
                        ScenarioSendTxsCliArgs {
                            testfile,
                            rpc_url,
                            private_keys,
                            min_balance,
                            seed,
                            tx_type,
                        },
                },
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
            args:
                SpamCliArgs {
                    eth_json_rpc_args:
                        ScenarioSendTxsCliArgs {
                            testfile,
                            rpc_url,
                            seed,
                            private_keys,
                            min_balance,
                            tx_type,
                        },
                    spam_args:
                        SendSpamCliArgs {
                            duration,
                            txs_per_block,
                            txs_per_second,
                            builder_url,
                        },
                    disable_reporting,
                    gen_report,
                    gas_price_percent_add,
                },
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
                    disable_reporting,
                    min_balance,
                    tx_type: tx_type.into(),
                    gas_price_percent_add,
                },
            )
            .await?;
            if gen_report {
                commands::report(run_id, 0, &db, &rpc_url).await?;
            }
        }

        ContenderSubcommand::SpamD {
            args:
                SpamCliArgs {
                    eth_json_rpc_args:
                        ScenarioSendTxsCliArgs {
                            testfile,
                            rpc_url,
                            seed,
                            private_keys,
                            min_balance,
                            tx_type,
                        },
                    spam_args:
                        SendSpamCliArgs {
                            duration,
                            txs_per_block,
                            txs_per_second,
                            builder_url,
                        },
                    disable_reporting,
                    gen_report,
                    gas_price_percent_add,
                },
        } => {
            let seed = seed.unwrap_or(stored_seed);

            // collects all run IDs for reporting
            let mut run_ids = vec![];
            let db = Arc::new(db);

            // run spam command with duration 1 in an async loop
            // trigger spam batch once per second

            let do_thing = || async move {
                for _ in 0..duration.unwrap_or_default() {
                    let args = SpamCommandArgs {
                        testfile: testfile.clone(),
                        rpc_url: rpc_url.clone(),
                        builder_url: builder_url.clone(),
                        txs_per_block,
                        txs_per_second,
                        duration: Some(1),
                        seed: seed.clone(),
                        private_keys: private_keys.clone(),
                        disable_reporting,
                        min_balance: min_balance.clone(),
                        tx_type: tx_type.into(),
                        gas_price_percent_add,
                    };
                    let db = db.clone();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let spam_res = commands::spam(&*db, args).await;
                    if let Err(e) = spam_res {
                        println!("spam failed: {:?}", e);
                    } else {
                        println!("spam batch completed");
                        let run_id = spam_res.expect("spam");
                        if let Some(run_id) = run_id {
                            run_ids.push(run_id);
                        }
                    }
                }

                if gen_report {
                    let first_run_id = *run_ids.iter().min().expect("no run IDs found");
                    let last_run_id = *run_ids.iter().max().expect("no run IDs found");
                    commands::report(
                        Some(last_run_id),
                        last_run_id - first_run_id,
                        &*db,
                        &rpc_url,
                    )
                    .await
                    .map_err(|e| {
                        ContenderError::GenericError("failed to generate report", e.to_string())
                    })?;
                }
                Ok::<_, ContenderError>(())
            };

            tokio::select! {
                _ = do_thing() => {},
                _ = tokio::signal::ctrl_c() => {
                    println!("CTRL-C received, hard-stopping spam daemon...");
                }
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
