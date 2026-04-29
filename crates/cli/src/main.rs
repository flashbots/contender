use alloy::{
    network::AnyNetwork,
    providers::{DynProvider, ProviderBuilder},
    rpc::client::ClientBuilder,
};
use contender_cli::commands;
use contender_cli::{
    commands::{
        admin::handle_admin_command,
        common::ScenarioSendTxsCliArgs,
        db::{drop_db, export_db, import_db, reset_db},
        error::ArgsError,
        replay::ReplayArgs,
        ContenderCli, ContenderSubcommand, DbCommand, ReportFormat, SetupCommandArgs,
        SpamCampaignContext, SpamCliArgs, SpamCommandArgs, SpamScenario,
    },
    default_scenarios::{fill_block::FillBlockCliArgs, BuiltinScenarioCli},
    util::{db_file_in, init_reports_dir, resolve_data_dir},
    Error,
};
use contender_core::{db::DbOps, util::TracingOptions};
use contender_report::command::ReportParams;
use contender_sqlite::{SqliteDb, DB_VERSION};
use regex::Regex;
use std::str::FromStr;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> miette::Result<()> {
    init_tokio_metrics();
    run().await.map_err(|e| e.into())
}

/// Initializes tokio metrics collection/logging thread.
/// No-op if the "tokio-metrics" feature is disabled.
fn init_tokio_metrics() {
    #[cfg(feature = "tokio-metrics")]
    {
        use std::time::Duration;
        use tokio_metrics::RuntimeMonitor;
        let handle = tokio::runtime::Handle::current();
        let monitor = RuntimeMonitor::new(&handle);
        tokio::spawn(async move {
            for interval in monitor.intervals() {
                // pretty-print the metric interval
                println!("{:#?}", interval);
                // wait
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        });
    }
}

async fn run() -> Result<(), contender_cli::Error> {
    let args = ContenderCli::parse_args();

    // The server subcommand initializes its own tracing subscriber with a
    // custom SessionLogRouter layer, so skip the default CLI tracing setup.
    if !matches!(args.command, ContenderSubcommand::Server) {
        init_tracing();
    }

    // Resolve data directory from CLI arg, env var, or default
    let data_dir = resolve_data_dir(args.data_dir.clone())?;
    let db_path = db_file_in(&data_dir);
    init_reports_dir(&data_dir);

    info!("data directory: {}", data_dir.display());
    debug!("opening DB at {db_path:?}");

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
            let args = SetupCommandArgs::new(scenario, args.rpc_args, &data_dir)?;

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
                SpamScenario::Builtin(
                    config
                        .to_builtin_scenario(&provider, args.builtin_options(&data_dir)?)
                        .await?,
                )
            } else {
                // default to fill-block scenario
                SpamScenario::Builtin(
                    BuiltinScenarioCli::FillBlock(FillBlockCliArgs {
                        max_gas_per_block: None,
                    })
                    .to_builtin_scenario(&provider, args.builtin_options(&data_dir)?)
                    .await?,
                )
            };

            let spam_args = SpamCommandArgs::new(scenario, *args, &data_dir)?;
            commands::spam(&db, &spam_args, SpamCampaignContext::default()).await?;
        }

        ContenderSubcommand::Server => {
            contender_cli::server::run()
                .await
                .map_err(|e| contender_cli::Error::ServerStartup(e.to_string()))?;
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
            skip_tx_traces,
            time_to_inclusion_bucket,
        } => {
            let use_json = matches!(format, ReportFormat::Json);
            if let Some(campaign_id) = campaign_id {
                let resolved_campaign_id = if campaign_id == "__LATEST_CAMPAIGN__" {
                    db.latest_campaign_id().map_err(Error::Db)?.ok_or_else(|| {
                        Error::Report(contender_report::Error::CampaignNotFound(
                            "latest".to_string(),
                        ))
                    })?
                } else {
                    campaign_id
                };
                if preceding_runs > 0 {
                    warn!("--preceding-runs is ignored when --campaign is provided");
                }
                let report_params = ReportParams::new()
                    .with_skip_tx_traces(skip_tx_traces)
                    .with_time_to_inclusion_bucket(time_to_inclusion_bucket)
                    .with_use_json(use_json);
                contender_report::command::report_campaign(
                    &resolved_campaign_id,
                    &db,
                    &data_dir,
                    report_params,
                )
                .await
                .map_err(Error::Report)?;
            } else {
                let use_json = matches!(format, ReportFormat::Json);
                let mut report_params = ReportParams::new()
                    .with_preceding_runs(preceding_runs)
                    .with_skip_tx_traces(skip_tx_traces)
                    .with_time_to_inclusion_bucket(time_to_inclusion_bucket)
                    .with_use_json(use_json);
                if let Some(last_run_id) = last_run_id {
                    report_params = report_params.with_last_run_id(last_run_id);
                }
                contender_report::command::report(&db, &data_dir, report_params).await?;
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

        ContenderSubcommand::Rpc { args } => {
            commands::rpc::run_rpc_spam(*args, &db, &data_dir).await?;
        }
    };

    Ok(())
}

/// Check DB version, throw error if version is incompatible with currently-running version of contender.
fn init_db(command: &ContenderSubcommand, db: &SqliteDb) -> Result<(), Error> {
    if db.table_exists("run_txs").map_err(Error::Db)? {
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
            return Err(Error::DbVersion);
        }
    } else {
        info!("no DB found, creating new DB");
        db.create_tables().map_err(Error::Db)?;
    }
    Ok(())
}

/// Reads the RUST_LOG environment variable and extracts log levels.
pub fn read_rust_log() -> Vec<tracing::Level> {
    let rustlog = std::env::var("RUST_LOG").unwrap_or_default().to_lowercase();

    // interpret log levels from words matching `=[a-zA-Z]+`
    let level_regex = Regex::new(r"=[a-zA-Z]+").unwrap();
    level_regex
        .find_iter(&rustlog)
        .map(|m| m.as_str().trim_start_matches('='))
        .map(|m| tracing::Level::from_str(m).unwrap_or(tracing::Level::INFO))
        .collect()
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().ok(); // fallback if RUST_LOG is unset
    let mut opts = TracingOptions::default();

    // if user provides any log level > info, print line num & source file in logs
    if read_rust_log()
        .iter()
        .any(|lvl| *lvl > tracing::Level::INFO)
    {
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
