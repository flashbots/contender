mod commands;
mod default_scenarios;
mod error;
mod util;

use crate::commands::{error::ArgsError, ReportFormat, SpamCampaignContext};
use alloy::{
    network::AnyNetwork,
    providers::{DynProvider, ProviderBuilder},
    rpc::client::ClientBuilder,
};
use commands::{
    admin::handle_admin_command,
    common::ScenarioSendTxsCliArgs,
    db::{drop_db, export_db, import_db, reset_db},
    replay::ReplayArgs,
    ContenderCli, ContenderSubcommand, DbCommand, SetupCommandArgs, SpamCliArgs, SpamCommandArgs,
    SpamScenario,
};
use contender_core::{db::DbOps, util::TracingOptions};
use contender_sqlite::{SqliteDb, DB_VERSION};
use default_scenarios::{fill_block::FillBlockCliArgs, BuiltinScenarioCli};
use error::CliError;
use regex::Regex;
use std::str::FromStr;
use tokio::sync::OnceCell;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use util::{db_file_in, init_reports_dir, resolve_data_dir};

// prometheus
static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
static LATENCY_HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();

#[tokio::main(flavor = "multi_thread")]
async fn main() -> miette::Result<()> {
    run().await.map_err(|e| e.into())
}

async fn run() -> Result<(), CliError> {
    init_tracing();

    let args = ContenderCli::parse_args();

    // Resolve data directory from CLI arg, env var, or default
    let data_dir = resolve_data_dir(args.data_dir.clone())?;
    let db_path = db_file_in(&data_dir);
    init_reports_dir(&data_dir);

    debug!("data directory: {data_dir}");
    debug!("opening DB at {db_path}");

    let db = SqliteDb::from_file(&db_path)?;
    init_db(&args.command, &db)?;

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
            let args = SetupCommandArgs::new(scenario, args.rpc_args)?;

            commands::setup(&db, args).await?
        }

        ContenderSubcommand::Spam {
            args,
            builtin_scenario_config,
        } => {
            if builtin_scenario_config.is_some() && args.eth_json_rpc_args.testfile.is_some() {
                return Err(ArgsError::ScenarioFileBuiltinConflict.into());
            }

            let SpamCliArgs {
                eth_json_rpc_args:
                    ScenarioSendTxsCliArgs {
                        testfile, rpc_args, ..
                    },
                ..
            } = *args.to_owned();

            let client = ClientBuilder::default().http(rpc_args.rpc_url.clone());
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

            let spam_args = SpamCommandArgs::new(scenario, *args)?;
            commands::spam(&db, &spam_args, SpamCampaignContext::default()).await?;
        }

        ContenderSubcommand::Replay { args } => {
            let args = ReplayArgs::from_cli_args(*args).await?;
            commands::replay::replay(args, &db).await?;
        }

        ContenderSubcommand::Report {
            last_run_id,
            preceding_runs,
            campaign_id,
            format,
        } => {
            if let Some(campaign_id) = campaign_id {
                let resolved_campaign_id = if campaign_id == "__LATEST_CAMPAIGN__" {
                    db.latest_campaign_id()
                        .map_err(CliError::Db)?
                        .ok_or_else(|| {
                            CliError::Report(contender_report::Error::CampaignNotFound(
                                "latest".to_string(),
                            ))
                        })?
                } else {
                    campaign_id
                };
                if preceding_runs > 0 {
                    warn!("--preceding-runs is ignored when --campaign is provided");
                }
                contender_report::command::report_campaign(&resolved_campaign_id, &db, &data_dir)
                    .await
                    .map_err(CliError::Report)?;
            } else {
                let use_json = matches!(format, ReportFormat::Json);
                contender_report::command::report(
                    last_run_id,
                    preceding_runs,
                    &db,
                    &data_dir,
                    use_json,
                )
                .await
                .map_err(CliError::Report)?;
            }
        }

        ContenderSubcommand::Admin { command } => {
            handle_admin_command(command, &data_dir, db).await?;
        }

        ContenderSubcommand::Campaign { args } => {
            tokio::select! {
                res = commands::campaign::run_campaign(&db, &data_dir, *args) => {
                    res?;
                }
                _ = tokio::signal::ctrl_c() => {
                    warn!("CTRL-C received, campaign terminated.");
                }
            }
        }
    };

    Ok(())
}

/// Check DB version, throw error if version is incompatible with currently-running version of contender.
fn init_db(command: &ContenderSubcommand, db: &SqliteDb) -> Result<(), CliError> {
    if db.table_exists("run_txs").map_err(CliError::Db)? {
        // check version and exit if DB version is incompatible
        let quit_early = db.version() != DB_VERSION
            && !matches!(
                command,
                ContenderSubcommand::Db { command: _ } | ContenderSubcommand::Admin { command: _ }
            );
        if quit_early {
            let recommendation = format!(
                "To backup your data, run `contender db export`.\n{}",
                if db.version() < DB_VERSION {
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
                db.version(),
                DB_VERSION
            );
            warn!("{recommendation}");
            return Err(CliError::DbVersion);
        }
    } else {
        info!("no DB found, creating new DB");
        db.create_tables().map_err(CliError::Db)?;
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().ok(); // fallback if RUST_LOG is unset

    let mut opts = TracingOptions::default();
    let rustlog = std::env::var("RUST_LOG").unwrap_or_default().to_lowercase();

    // interpret log levels from words matching `=[a-zA-Z]+`
    let level_regex = Regex::new(r"=[a-zA-Z]+").unwrap();
    let matches: Vec<tracing::Level> = level_regex
        .find_iter(&rustlog)
        .map(|m| m.as_str().trim_start_matches('='))
        .map(|m| tracing::Level::from_str(m).unwrap_or(tracing::Level::INFO))
        .collect();

    // if user provides any log level > info, print line num & source file in logs
    if matches.iter().any(|lvl| *lvl > tracing::Level::INFO) {
        opts = opts.with_line_number(true).with_target(true);
    }

    #[cfg(feature = "async-tracing")]
    {
        use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, Layer};
        let tokio_layer = console_subscriber::ConsoleLayer::builder()
            .with_default_env()
            .spawn();
        let fmt_layer = fmt::layer()
            .with_ansi(opts.ansi)
            .with_target(opts.target)
            .with_line_number(opts.line_number)
            .with_filter(filter);

        tracing_subscriber::Registry::default()
            .with(fmt_layer)
            .with(tokio_layer)
            .init();
    }

    #[cfg(not(feature = "async-tracing"))]
    {
        contender_core::util::init_core_tracing(filter, opts);
    }
}
