use clap::Subcommand;
use std::path::PathBuf;

use super::admin::AdminCommand;
use super::setup::SetupCliArgs;
use super::spam::SpamCliArgs;

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

    #[command(
        name = "spam",
        long_about = "Spam the RPC with tx requests as designated in the given testfile."
    )]
    Spam {
        #[command(flatten)]
        args: SpamCliArgs,
    },

    #[command(
        name = "setup",
        long_about = "Deploy contracts and run the setup step(s) in the given testfile."
    )]
    Setup {
        #[command(flatten)]
        args: SetupCliArgs,
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
