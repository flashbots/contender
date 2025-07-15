mod commands;
mod default_scenarios;
mod util;

use alloy::{
    hex,
    network::AnyNetwork,
    providers::{DynProvider, ProviderBuilder},
    rpc::client::ClientBuilder,
    transports::http::reqwest::Url,
};
use commands::{
    admin::handle_admin_command,
    common::{ScenarioSendTxsCliArgs, SendSpamCliArgs},
    db::{drop_db, export_db, import_db, reset_db},
    ContenderCli, ContenderSubcommand, DbCommand, SetupCommandArgs, SpamCliArgs, SpamCommandArgs,
    SpamScenario,
};
use console_subscriber;
use contender_core::{db::DbOps, error::ContenderError, generator::RandSeed};
use contender_sqlite::{SqliteDb, DB_VERSION};
use default_scenarios::{fill_block::FillBlockCliArgs, BuiltinScenarioCli};
use rand::Rng;
use std::{str::FromStr, sync::LazyLock};
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use util::{data_dir, db_file, prompt_continue};

use crate::util::{bold, init_reports_dir};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    let path = db_file().expect("failed to get DB file path");
    debug!("opening DB at {path}");
    SqliteDb::from_file(&path).expect("failed to open contender DB file")
});
// prometheus
static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
static LATENCY_HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    init_reports_dir();

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
            warn!("Your database is incompatible with this version of contender.");
            warn!(
                "Remote DB version = {}, contender expected version {}.",
                DB.version(),
                DB_VERSION
            );
            warn!("{recommendation}");
            return Err("Incompatible DB detected".into());
        }
    } else {
        info!("no DB found, creating new DB");
        DB.create_tables()?;
    }
    let db = DB.clone();
    let data_path = data_dir()?;
    let db_path = db_file()?;

    let seed_path = format!("{}/seed", &data_path);
    if !std::path::Path::new(&seed_path).exists() {
        info!("generating seed file at {}", &seed_path);
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

        ContenderSubcommand::Setup { args } => {
            let ScenarioSendTxsCliArgs {
                testfile,
                rpc_url,
                private_keys,
                min_balance,
                seed,
                tx_type,
                bundle_type,
                auth_args,
                env,
            } = args.args;
            let seed = seed.unwrap_or(stored_seed);
            let engine_params = auth_args.engine_params().await?;
            let testfile = if let Some(testfile) = testfile {
                testfile
            } else {
                // if no testfile is provided, use the default one
                warn!("No testfile provided, using default testfile \"scenario:simple.toml\"");
                "scenario:simple.toml".to_owned()
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
                    bundle_type: bundle_type.into(),
                    engine_params,
                    env,
                },
            )
            .await?
        }

        ContenderSubcommand::Spam {
            args,
            builtin_scenario_config,
        } => {
            if !check_spam_args(&args)? {
                return Ok(());
            }
            if builtin_scenario_config.is_some() && args.eth_json_rpc_args.testfile.is_some() {
                return Err(ContenderError::SpamError(
                    "Cannot use both builtin scenario and testfile",
                    None,
                )
                .into());
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
                        bundle_type,
                        auth_args,
                        env,
                    },
                spam_args,
                disable_reporting,
                gen_report,
                ..
            } = *args.to_owned();

            let SendSpamCliArgs {
                builder_url,
                txs_per_block,
                txs_per_second,
                duration,
                timeout,
                loops,
                ..
            } = spam_args.to_owned();

            let seed = seed.unwrap_or(stored_seed);
            let engine_params = auth_args.engine_params().await?;
            let client = ClientBuilder::default().http(Url::from_str(&rpc_url)?);
            let provider = DynProvider::new(
                ProviderBuilder::new()
                    .network::<AnyNetwork>()
                    .connect_client(client),
            );

            let scenario = if let Some(testfile) = testfile {
                SpamScenario::Testfile(testfile)
            } else if let Some(config) = builtin_scenario_config {
                SpamScenario::Builtin(config.to_builtin_scenario(&provider, &spam_args).await?)
            } else {
                // default to fill-block scenario
                SpamScenario::Builtin(
                    BuiltinScenarioCli::FillBlock(FillBlockCliArgs {
                        max_gas_per_block: None,
                    })
                    .to_builtin_scenario(&provider, &spam_args)
                    .await?,
                )
            };

            let real_loops = if let Some(loops) = loops {
                // loops flag is set; spamd will interpret a None value as infinite
                loops
            } else {
                // loops flag is not set, so only loop once
                Some(1)
            };
            let spam_args = SpamCommandArgs {
                scenario,
                rpc_url,
                builder_url,
                txs_per_block,
                txs_per_second,
                duration,
                seed,
                private_keys,
                disable_reporting,
                min_balance,
                tx_type: tx_type.into(),
                bundle_type: bundle_type.into(),
                engine_params,
                timeout_secs: timeout,
                env,
                loops: real_loops,
            };

            commands::spamd(&db, spam_args, gen_report, real_loops).await?;
        }

        ContenderSubcommand::Report {
            last_run_id,
            preceding_runs,
        } => {
            contender_report::command::report(
                last_run_id,
                preceding_runs,
                &db,
                &data_dir().expect("invalid data dir"),
            )
            .await?;
        }

        ContenderSubcommand::Admin { command } => {
            handle_admin_command(command, db)?;
        }
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")); // fallback if RUST_LOG is unset

    let tokio_layer = console_subscriber::ConsoleLayer::builder()
        .with_default_env()
        .spawn();
    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_line_number(true)
        .with_filter(filter);

    tracing_subscriber::Registry::default()
        .with(fmt_layer)
        .with(tokio_layer)
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
            "Missing params.",
            Some(format!(
                "Either {} or {} must be set.",
                bold("--txs-per-block (--tpb)"),
                bold("--txs-per-second (--tps)"),
            )),
        ));
    };
    let duration = args.spam_args.duration;
    if duration > max_duration {
        let time_limit = duration / max_duration;
        let scenario = args
            .eth_json_rpc_args
            .testfile
            .as_deref()
            .unwrap_or_default();
        let suggestion_cmd = ansi_term::Style::new().bold().paint(format!(
            "contender spam {scenario} -d {max_duration} -l {time_limit} ..."
        ));
        println!(
"Duration is set to {duration} {units}, which is quite high. Generating transactions and collecting results may take a long time.
You may want to use {} with a lower spamming duration {} and a loop limit {}:\n
\t{suggestion_cmd}\n",
            bold("spam"),
            bold("(-d)"),
            bold("(-l)")
    );
        return Ok(prompt_continue(None));
    }
    Ok(true)
}
