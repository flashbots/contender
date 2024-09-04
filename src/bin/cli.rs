use clap::{Parser, Subcommand};
use contender_core::spammer::spam_rpc;

#[derive(Parser, Debug)]
struct ContenderCli {
    #[command(subcommand)]
    command: ContenderSubcommand,
}

#[derive(Debug, Subcommand)]
enum ContenderSubcommand {
    #[command(name = "spam", long_about = "Spam the RPC with tx requests.")]
    Spam {
        /// The path to the test file to use for spamming.
        testfile: String,

        /// The RPC URL to spam with requests.
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse();
    match args.command {
        ContenderSubcommand::Spam {
            testfile,
            rpc_url,
            intensity,
            duration,
        } => {
            spam_rpc(
                &testfile,
                &rpc_url,
                intensity.unwrap_or_default(),
                duration.unwrap_or_default(),
            )?;
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
