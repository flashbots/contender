use clap::Subcommand;
use std::path::PathBuf;

use super::setup::SetupCliArgs;
use super::spam::SpamCliArgs;
use crate::default_scenarios::BuiltinScenario;
use crate::util::TxTypeCli;

#[derive(Debug, Subcommand)]
pub enum ContenderSubcommand {
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
        name = "spamd",
        long_about = "Run spam in a loop over a long duration or indefinitely."
    )]
    SpamD {
        #[command(flatten)]
        spam_inner_args: SpamCliArgs,

        #[arg(
            short = 'l',
            long,
            long_help = "The time limit in seconds for the spam daemon. If not provided, the daemon will run indefinitely.",
            visible_aliases = &["tl"]
        )]
        time_limit: Option<u64>,
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
        /// The HTTP JSON-RPC URL to use for setup.
        rpc_url: String,

        /// The run ID to include in the report.
        #[arg(
            short = 'i',
            long,
            long_help = "The first run to include in the report. If not provided, the latest run is used."
        )]
        last_run_id: Option<u64>,

        /// The number of runs preceding `last_run_id` to include in the report.
        /// If not provided, only the run with ID `last_run_id` is included.
        #[arg(
            short,
            long,
            long_help = "The number of runs preceding `last_run_id` to include in the report. If not provided, only the run with ID `end_run_id` is included.",
            default_value = "0"
        )]
        preceding_runs: u64,
    },

    #[command(name = "run", long_about = "Run a builtin scenario.")]
    Run {
        /// The scenario to run.
        scenario: BuiltinScenario,

        /// The HTTP JSON-RPC URL to target with the scenario.
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

        /// Auth RPC URL for `engine_` calls
        #[arg(
                    long,
                    long_help = "Provide this URL to enable use of engine_ calls.",
                    visible_aliases = &["auth"]
                )]
        auth_rpc_url: Option<String>,

        /// JWT secret used for `engine_` calls
        #[arg(
                    long,
                    long_help = "JWT secret.
        Required if --auth-rpc-url is set.",
                    visible_aliases = &["jwt"]
                )]
        jwt_secret: Option<String>,

        /// Call `engine_forkchoiceUpdated` after each block
        #[arg(
                    long,
                    long_help = "Call engine_forkchoiceUpdated on Auth RPC after each block.
        Requires --auth-rpc-url and --jwt-secret to be set.",
                    visible_aliases = &["fcu"]
                )]
        call_forkchoice: bool,
        // TODO: DRY duplicate args
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
