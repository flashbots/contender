use clap::Subcommand;
use std::path::PathBuf;

use super::admin::AdminCommand;
use super::common::AuthCliArgs;
use super::setup::SetupCliArgs;
use super::spam::SpamCliArgs;
use crate::default_scenarios::BuiltinScenario;
use crate::util::TxTypeCli;

#[derive(Debug, Subcommand)]
pub enum ContenderSubcommand {
    #[command(name = "compose", about = "Composite scenario")]
    Composite {
        #[arg(
            short,
            long,
            long_help = "File path for composite scenario file",
            default_value = "./contender-compose.yml"
        )]
        filename: Option<String>,
    },

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

    #[command(name = "run", long_about = "Run a builtin scenario.")]
    Run {
        /// The scenario to run.
        scenario: BuiltinScenario,

        /// The HTTP JSON-RPC URL to spam with requests.
        #[arg(
            short,
            long,
            long_help = "RPC URL to test the scenario.",
            default_value = "http://localhost:8545"
        )]
        rpc_url: String,

        #[arg(
            short,
            long = "priv-key",
            long_help = "Private key used to send all transactions."
        )]
        private_key: Option<String>,

        #[arg(
            short,
            long = "interval",
            long_help = "Interval in seconds between each batch of requests.",
            default_value = "1"
        )]
        interval: f64,

        #[arg(
            short,
            long = "duration",
            long_help = "The number of batches of requests to send.",
            default_value = "10"
        )]
        duration: u64,

        #[arg(
            short = 'n',
            long = "num-txs",
            long_help = "The number of txs to send on each elapsed interval.",
            default_value = "50"
        )]
        txs_per_duration: u64,

        #[arg(
            long,
            long_help = "Skip the deploy prompt. Contracts will only be deployed if not found in DB.",
            visible_aliases = &["sdp"]
        )]
        skip_deploy_prompt: bool,

        /// Transaction type
        #[arg(
            short = 't',
            long,
            long_help = "Transaction type for all transactions.",
            value_enum,
            default_value_t = TxTypeCli::Eip1559,
        )]
        tx_type: TxTypeCli,

        #[command(flatten)]
        auth_args: AuthCliArgs,
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
