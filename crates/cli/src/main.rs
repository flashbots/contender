mod commands;
mod default_scenarios;
mod error;
mod util;

use crate::commands::error::ArgsError;
use alloy::{
    network::AnyNetwork,
    providers::{DynProvider, ProviderBuilder},
    rpc::client::ClientBuilder,
    transports::http::reqwest::Url,
};
use commands::{
    admin::handle_admin_command,
    common::{ScenarioSendTxsCliArgs, SendSpamCliArgs},
    db::{drop_db, export_db, import_db, reset_db},
    replay::ReplayArgs,
    ContenderCli, ContenderSubcommand, DbCommand, SetupCommandArgs, SpamCliArgs, SpamCommandArgs,
    SpamScenario,
};
use contender_core::db::DbOps;
use contender_sqlite::{SqliteDb, DB_VERSION};
use default_scenarios::{fill_block::FillBlockCliArgs, BuiltinScenarioCli};
use error::ContenderError;
use std::{str::FromStr, sync::LazyLock};
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use util::{bold, data_dir, db_file, init_reports_dir, prompt_continue};

static DB: LazyLock<SqliteDb> = std::sync::LazyLock::new(|| {
    let path = db_file().expect("failed to get DB file path");
    debug!("opening DB at {path}");
    SqliteDb::from_file(&path).expect("failed to open contender DB file")
});
// prometheus
static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
static LATENCY_HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();

#[tokio::main(flavor = "multi_thread")]
async fn main() -> miette::Result<()> {
    run().await.map_err(|e| e.into())
}

async fn run() -> Result<(), ContenderError> {
    init_tracing();
    init_reports_dir();

    let args = ContenderCli::parse_args();
    init_db(&args.command)?;
    let db = DB.clone();
    let db_path = db_file()?;

    match args.command {
        ContenderSubcommand::Db { command } => match command {
            DbCommand::Drop => drop_db(&db_path).await?,
            DbCommand::Reset => reset_db(&db_path).await?,
            DbCommand::Export { out_path } => export_db(&db_path, out_path).await?,
            DbCommand::Import { src_path } => import_db(src_path, &db_path).await?,
        },

        ContenderSubcommand::Setup { args } => {
            let testfile = if let Some(testfile) = &args.testfile {
                testfile
            } else {
                // if no testfile is provided, use the default one
                warn!("No testfile provided, using default testfile \"scenario:simple.toml\"");
                "scenario:simple.toml"
            };
            let scenario = SpamScenario::Testfile(testfile.to_owned());
            let args = SetupCommandArgs::new(scenario, *args)?;

            commands::setup(&db, args).await?
        }

        ContenderSubcommand::Spam {
            args,
            builtin_scenario_config,
        } => {
            if !check_spam_args(&args)? {
                return Ok(());
            }
            if builtin_scenario_config.is_some() && args.eth_json_rpc_args.testfile.is_some() {
                return Err(ArgsError::ScenarioFileBuiltinConflict.into());
            }

            let SpamCliArgs {
                eth_json_rpc_args:
                    ScenarioSendTxsCliArgs {
                        testfile, rpc_url, ..
                    },
                spam_args,
                gen_report,
                ..
            } = *args.to_owned();

            let SendSpamCliArgs { loops, .. } = spam_args.to_owned();

            let client = ClientBuilder::default()
                .http(Url::from_str(&rpc_url).map_err(ArgsError::UrlParse)?);
            let provider = DynProvider::new(
                ProviderBuilder::new()
                    .network::<AnyNetwork>()
                    .connect_client(client),
            );

            let scenario = if let Some(testfile) = testfile {
                SpamScenario::Testfile(testfile)
            } else if let Some(config) = builtin_scenario_config {
                SpamScenario::Builtin(config.to_builtin_scenario(&provider, &args).await?)
            } else {
                // default to fill-block scenario
                SpamScenario::Builtin(
                    BuiltinScenarioCli::FillBlock(FillBlockCliArgs {
                        max_gas_per_block: None,
                    })
                    .to_builtin_scenario(&provider, &args)
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
            let spamd_args = SpamCommandArgs::new(scenario, *args)?;
            commands::spamd(&db, spamd_args, gen_report, real_loops).await?;
        }

        ContenderSubcommand::Replay { args } => {
            let args = ReplayArgs::from_cli_args(*args).await?;
            commands::replay::replay(args, &DB.clone()).await?;
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
            .await
            .map_err(ContenderError::Report)?;
        }

        ContenderSubcommand::Admin { command } => {
            handle_admin_command(command, db).await?;
        }
    };

    Ok(())
}

/// Check DB version, throw error if version is incompatible with currently-running version of contender.
fn init_db(command: &ContenderSubcommand) -> Result<(), ContenderError> {
    if DB.table_exists("run_txs").map_err(ContenderError::Db)? {
        // check version and exit if DB version is incompatible
        let quit_early = DB.version() != DB_VERSION
            && !matches!(
                command,
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
            return Err(ContenderError::DbVersion);
        }
    } else {
        info!("no DB found, creating new DB");
        DB.create_tables().map_err(ContenderError::Db)?;
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().ok(); // fallback if RUST_LOG is unset
    #[cfg(feature = "async-tracing")]
    {
        use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, Layer};
        let tokio_layer = console_subscriber::ConsoleLayer::builder()
            .with_default_env()
            .spawn();
        let fmt_layer = fmt::layer()
            .with_ansi(true)
            .with_target(true)
            .with_line_number(true)
            .with_filter(filter);

        tracing_subscriber::Registry::default()
            .with(fmt_layer)
            .with(tokio_layer)
            .init();
    }

    #[cfg(not(feature = "async-tracing"))]
    {
        contender_core::util::init_core_tracing(filter);
    }
}

/// Check if spam arguments are typical and prompt the user to continue if they are not.
/// Returns true if the user chooses to continue, false otherwise.
fn check_spam_args(args: &SpamCliArgs) -> Result<bool, ContenderError> {
    let (units, max_duration) = if args.spam_args.txs_per_block.is_some() {
        ("blocks", 50)
    } else if args.spam_args.txs_per_second.is_some() {
        ("seconds", 100)
    } else {
        return Err(ArgsError::SpamRateNotFound.into());
    };
    let duration = args.spam_args.duration;
    if duration > max_duration {
        let time_limit = duration / max_duration;
        let scenario = args
            .eth_json_rpc_args
            .testfile
            .as_deref()
            .unwrap_or_default();
        let suggestion_cmd = bold(format!(
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
