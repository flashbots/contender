use clap::{Parser, Subcommand};
use contender_core::{
    generator::{
        test_config::{SetupGenerator, TestConfig, TestGenerator},
        RandSeed,
    },
    spammer::Spammer,
};

#[derive(Parser, Debug)]
struct ContenderCli {
    #[command(subcommand)]
    command: ContenderSubcommand,
}

#[derive(Debug, Subcommand)]
enum ContenderSubcommand {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse();
    match args.command {
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            intensity,
            duration,
            seed,
        } => {
            let testfile = TestConfig::from_file(&testfile)?;
            let rand_seed = seed.map(|s| RandSeed::from_str(&s)).unwrap_or_default();
            let testgen = TestGenerator::new(testfile, &rand_seed);
            let spammer = Spammer::new(testgen, rpc_url);
            spammer.spam_rpc(intensity.unwrap_or_default(), duration.unwrap_or_default())?;
        }
        ContenderSubcommand::Setup { testfile, rpc_url } => {
            let gen: SetupGenerator = TestConfig::from_file(&testfile)?.into();
            let spammer = Spammer::new(gen, rpc_url);
            spammer.spam_rpc(10, 1)?;
        }
        ContenderSubcommand::Report { id, out_file } => {
            println!(
                "Exporting report for run ID {:?} to out_file {:?}",
                id, out_file
            );
            todo!();
        }
    }
    Ok(())
}
