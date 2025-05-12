mod commands;
mod default_scenarios;
mod util;

use alloy::hex;
use commands::{
    admin::handle_admin_command,
    common::{ScenarioSendTxsCliArgs, SendSpamCliArgs},
    db::{drop_db, export_db, import_db, reset_db},
    ContenderCli, ContenderSubcommand, DbCommand, RunCommandArgs, SetupCliArgs, SetupCommandArgs,
    SpamCliArgs, SpamCommandArgs,
};
use contender_core::{db::DbOps, error::ContenderError, generator::RandSeed};
use contender_sqlite::{SqliteDb, DB_VERSION};
use rand::Rng;
use std::sync::LazyLock;
use tokio::sync::OnceCell;
use tracing_subscriber::EnvFilter;
use util::{data_dir, db_file, prompt_continue};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    let path = db_file().expect("failed to get DB file path");
    println!("opening DB at {path}");
    SqliteDb::from_file(&path).expect("failed to open contender DB file")
});
// prometheus
static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
static LATENCY_HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let args = ContenderCli::parse_args();
    if DB.table_exists("run_txs")? {
        // check version and exit if DB version is incompatible
        let quit_early = DB.version() != DB_VERSION
            && !matches!(
                &args.command,
                ContenderSubcommand::Db { command: _ } | ContenderSubcommand::Admin { command: _ }
            );
        if quit_early {
            let recommendation = format!(
                "To backup your data, run `contender db export`.\n{}",
                if DB.version() < DB_VERSION {
                    // contender version is newer than DB version, so user needs to upgrade DB
                    "Please run `contender db drop` or `contender db reset` to update your DB."
                } else {
                    // DB version is newer than contender version, so user needs to downgrade DB or upgrade contender
                    "Please upgrade contender or run `contender db drop` to delete your DB."
                }
            );
            println!(
                "Your database is incompatible with this version of contender.
Remote DB version = {}, contender expected version {}.

{recommendation}
",
                DB.version(),
                DB_VERSION
            );
            return Err("Incompatible DB detected".into());
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
            DbCommand::Drop => drop_db(&db_path).await?,
            DbCommand::Reset => reset_db(&db_path).await?,
            DbCommand::Export { out_path } => export_db(&db_path, out_path).await?,
            DbCommand::Import { src_path } => import_db(src_path, &db_path).await?,
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
                            auth_args,
                            env,
                        },
                },
        } => {
            let seed = seed.unwrap_or(stored_seed);
            let engine_params = auth_args.engine_params().await?;
            commands::setup(
                &db,
                SetupCommandArgs {
                    testfile,
                    rpc_url,
                    private_keys,
                    min_balance,
                    seed: RandSeed::seed_from_str(&seed),
                    tx_type: tx_type.into(),
                    engine_params,
                    env,
                },
            )
            .await?
        }

        ContenderSubcommand::Spam { args } => {
            if !check_spam_args(&args)? {
                return Ok(());
            }

            let SpamCliArgs {
                eth_json_rpc_args:
                    ScenarioSendTxsCliArgs {
                        testfile,
                        rpc_url,
                        seed,
                        private_keys,
                        min_balance,
                        tx_type,
                        auth_args,
                        env,
                    },
                spam_args:
                    SendSpamCliArgs {
                        duration,
                        txs_per_block,
                        txs_per_second,
                        builder_url,
                        timeout,
                        loops,
                    },
                disable_reporting,
                gen_report,
                gas_price_percent_add,
            } = args;

            let seed = seed.unwrap_or(stored_seed);
            let engine_params = auth_args.engine_params().await?;

            let spam_args = SpamCommandArgs {
                testfile: testfile.to_owned(),
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
                engine_params,
                timeout_secs: timeout,
                env,
            };
            let real_loops = if let Some(loops) = loops {
                // loops flag is set; spamd will interpret a None value as infinite
                loops
            } else {
                // loops flag is not set, so only loop once
                Some(1)
            };
            commands::spamd(&db, spam_args, gen_report, real_loops).await?;
        }

        ContenderSubcommand::Report {
            last_run_id,
            preceding_runs,
        } => {
            commands::report(last_run_id, preceding_runs, &db).await?;
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
            auth_args,
        } => {
            let engine_params = auth_args.engine_params().await?;
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
                    engine_params,
                },
            )
            .await?
        }

        ContenderSubcommand::Admin { command } => {
            handle_admin_command(command, db)?;
        }
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")); // fallback if RUST_LOG is unset

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_line_number(true)
        .init();
}

/// Check if spam arguments are typical and prompt the user to continue if they are not.
/// Returns true if the user chooses to continue, false otherwise.
fn check_spam_args(args: &SpamCliArgs) -> Result<bool, ContenderError> {
    let (units, max_duration) = if args.spam_args.txs_per_block.is_some() {
        ("blocks", 50)
    } else if args.spam_args.txs_per_second.is_some() {
        ("seconds", 100)
    } else {
        return Err(ContenderError::SpamError(
            "Either txs-per-block or txs-per-second must be set",
            None,
        ));
    };
    let duration = args.spam_args.duration;
    if duration > max_duration {
        let time_limit = duration / max_duration;
        let suggestion_cmd = ansi_term::Style::new().bold().paint(format!(
            "contender spam {} -d {max_duration} -l {time_limit} ...",
            args.eth_json_rpc_args.testfile
        ));
        println!(
"Duration is set to {duration} {units}, which is quite high. Generating transactions and collecting results may take a long time.
You may want to use {} with a lower spamming duration {} and a loop limit {}:\n
\t{suggestion_cmd}\n",
            ansi_term::Style::new().bold().paint("spam"),
            ansi_term::Style::new().bold().paint(format!("(-d)")),
            ansi_term::Style::new().bold().paint("(-l)")
    );
        return Ok(prompt_continue(None));
    }
    Ok(true)
}
