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

        /// The number of txs to send per second using the timed spammer. This is the default spammer.
        /// May not be set if `txs_per_block` is set.
        #[arg(long, long_help = "Number of txs to send per second. Must not be set if --txs-per-block is set.", visible_aliases = &["tps"])]
        txs_per_second: Option<usize>,

        /// The number of txs to send per block using the blockwise spammer.
        /// May not be set if `txs_per_second` is set. Requires `prv_keys` to be set.
        #[arg(
            long,
            long_help =
"Number of txs to send per block. Must not be set if --txs-per-second is set.
Requires --priv-key to be set for each 'from' address in the given testfile.",
        visible_aliases = &["tpb"])]
        txs_per_block: Option<usize>,

        /// The duration of the spamming run in seconds or blocks, depending on whether `txs_per_second` or `txs_per_block` is set.
        #[arg(
            short,
            long,
            default_value = "10",
            long_help = "Duration of the spamming run in seconds or blocks, depending on whether --txs-per-second or --txs-per-block is set."
        )]
        duration: Option<usize>,

        /// The seed to use for generating spam transactions. If not provided, one is generated.
        #[arg(
            short,
            long,
            long_help = "The seed to use for generating spam transactions"
        )]
        seed: Option<String>,

        /// The private keys to use for blockwise spamming.
        /// Required if `txs_per_block` is set.
        #[arg(
            short,
            long = "priv-key",
            long_help = "Add private keys for blockwise spamming. Required if --txs-per-block is set.
May be specified multiple times."
        )]
        private_keys: Option<Vec<String>>,

        /// Whether to log reports for the spamming run.
        #[arg(
            long,
            long_help = "Whether to log reports for the spamming run.",
            visible_aliases = &["dr"]
        )]
        disable_reports: bool,

        /// The minimum balance to check for each private key.
        #[arg(
            long,
            long_help = "The minimum balance to check for each private key in decimal-ETH format (`--min-balance 1.5` means 1.5 * 1e18 wei).",
            default_value = "1.0"
        )]
        min_balance: String,
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

        /// The private keys to use for setup.
        #[arg(
            short,
            long = "priv-key",
            long_help = "Add private keys used to deploy and setup contracts.
May be specified multiple times."
        )]
        private_keys: Option<Vec<String>>,

        /// The minimum balance to check for each private key.
        #[arg(
            long,
            long_help = "The minimum balance to check for each private key in decimal-ETH format (ex: `--min-balance 1.5` means 1.5 * 1e18 wei).",
            default_value = "1.0"
        )]
        min_balance: String,
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
        id: Option<u64>,

        /// The path to save the report to.
        /// If not provided, the report is saved to the current directory.
        #[arg(
            short,
            long,
            long_help = "Filename of the saved report. May be a fully-qualified path. If not provided, the report is saved to the current directory."
        )]
        out_file: Option<String>,
    },

    #[command(name = "template", long_about = "Generate a template testfile.")]
    Template {
        /// The path to save the template to.
        /// If not provided, the template is saved to the current directory.
        #[arg(
            short,
            long,
            long_help = "Filename of the saved template. May be a fully-qualified path. If not provided, the template is saved to the current directory."
        )]
        out_file: Option<String>,

        #[arg(
            short,
            long,
            long_help = "The base template to use for the generated testfile. Options: FillBlock, UniswapV2 (soon)."
        )]
        base_template: Option<String>,

        #[arg(
            short,
            long,
            long_help = "The RPC URL to fetch the gas limit from. If not provided, the gas limit is presumed to be 30000000."
        )]
        rpc_url: Option<String>,
    },
}
