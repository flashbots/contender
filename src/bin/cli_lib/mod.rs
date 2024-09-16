use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
pub struct ContenderCli {
    #[command(subcommand)]
    pub command: ContenderSubcommand,
}

impl ContenderCli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[derive(Debug, Subcommand)]
pub enum ContenderSubcommand {
    #[command(
        name = "spam",
        long_about = "Spam the RPC with tx requests as designated in the given testfile."
    )]
    Spam {
        /// The path to the test file to use for spamming.
        testfile: String,

        /// The HTTP JSON-RPC URL to spam with requests.
        rpc_url: String,

        /// The number of txs to send per second.
        #[arg(short, long, default_value = "10", long_help = "Number of txs to send per second", visible_aliases = &["tps"])]
        intensity: Option<usize>,

        /// The duration of the spamming run in seconds.
        #[arg(
            short,
            long,
            default_value = "60",
            long_help = "Duration of the spamming run in seconds"
        )]
        duration: Option<usize>,

        /// The seed to use for generating spam transactions. If not provided, one is generated.
        #[arg(
            short,
            long,
            long_help = "The seed to use for generating spam transactions"
        )]
        seed: Option<String>,
    },

    #[command(
        name = "setup",
        long_about = "Run the setup step(s) in the given testfile."
    )]
    Setup {
        /// The path to the test file to use for setup.
        testfile: String,

        /// The HTTP JSON-RPC URL to use for setup.
        rpc_url: String,
    },

    #[command(
        name = "report",
        long_about = "Export performance reports for data analysis."
    )]
    Report {
        /// The run ID to export reports for. If not provided, the latest run is used.
        #[arg(
            short,
            long,
            long_help = "The run ID to export reports for. If not provided, the latest run is used."
        )]
        id: Option<String>,

        /// The path to save the report to.
        /// If not provided, the report is saved to the current directory.
        #[arg(
            short,
            long,
            long_help = "Filename of the saved report. May be a fully-qualified path. If not provided, the report is saved to the current directory."
        )]
        out_file: Option<String>,
    },
}
