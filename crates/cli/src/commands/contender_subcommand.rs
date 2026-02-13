use clap::Subcommand;
use std::path::PathBuf;

use crate::commands::campaign::CampaignCliArgs;
use crate::commands::common::ScenarioSendTxsCliArgs;
use crate::commands::replay::ReplayCliArgs;
use crate::default_scenarios::BuiltinScenarioCli;

use super::admin::AdminCommand;
use super::spam::SpamCliArgs;
use super::ReportFormat;

#[derive(Debug, Subcommand)]
pub enum ContenderSubcommand {
    #[command(name = "admin", about = "Admin commands")]
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },

    #[command(name = "db", about = "Database management commands")]
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },

    #[command(name = "spam", long_about = "Spam the RPC with tx requests.")]
    Spam {
        #[command(flatten)]
        args: Box<SpamCliArgs>,

        #[command(subcommand, name = "builtin-scenario")]
        builtin_scenario_config: Option<BuiltinScenarioCli>,
    },

    #[command(
        name = "setup",
        long_about = "Deploy contracts and execute one-time setup txs."
    )]
    Setup {
        #[command(flatten)]
        args: Box<ScenarioSendTxsCliArgs>,
    },

    #[command(
        name = "replay",
        long_about = "Replay a range of blocks with the engine_ API."
    )]
    Replay {
        #[command(flatten)]
        args: Box<ReplayCliArgs>,
    },

    #[command(
        name = "report",
        long_about = "Export chain performance report for a spam run."
    )]
    Report {
        /// The run ID to include in the report.
        #[arg(
            short = 'i',
            long,
            long_help = "The first run to include in the report. If not provided, the latest run is used."
        )]
        last_run_id: Option<u64>,

        /// The number of runs preceding `last_run_id` to include in the report.
        /// Only runs with rpc_url matching the one in the last run are included.
        #[arg(
            short,
            long,
            long_help = "The number of runs preceding `last_run_id` to include in the report. Only runs with a RPC URL matching the last run will be included.",
            default_value = "0"
        )]
        preceding_runs: u64,

        /// Generate a campaign summary by campaign_id.
        #[arg(
            long,
            help = "Generate reports for all runs associated with the given campaign ID.",
            visible_alias = "campaign",
            conflicts_with = "last_run_id",
            value_name = "CAMPAIGN_ID",
            num_args = 0..=1,
            default_missing_value = "__LATEST_CAMPAIGN__"
        )]
        campaign_id: Option<String>,

        /// Output format: html (default, opens browser) or json (machine-readable).
        #[arg(
            long,
            short = 'f',
            default_value = "html",
            value_enum
        )]
        format: ReportFormat,
    },

    #[command(
        name = "campaign",
        long_about = "Run a composite/meta scenario described by a campaign file."
    )]
    Campaign {
        #[command(flatten)]
        args: Box<CampaignCliArgs>,
    },
}

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    #[command(name = "drop", about = "Delete the database file")]
    Drop,

    #[command(name = "reset", about = "Drop and re-initialize the database")]
    Reset,

    #[command(name = "export", about = "Save database to a new file")]
    Export {
        /// Path where to save the database file
        #[arg(help = "Path where to save the database file")]
        out_path: PathBuf,
    },

    #[command(name = "import", about = "Import database from a file")]
    Import {
        /// Path to the database file to import
        #[arg(help = "Path to the database file to import")]
        src_path: PathBuf,
    },
}
