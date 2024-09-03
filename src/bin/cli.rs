use clap::{Parser, Subcommand};
use contender_core::spam::spam_rpc;

#[derive(Parser, Debug)]
struct ContenderCli {
    #[command(subcommand)]
    command: ContenderSubcommand,
}

#[derive(Debug, Subcommand)]
enum ContenderSubcommand {
    #[command(name = "spam", long_about = "Spam the RPC with tx requests.")]
    Spam {
        /// The RPC URL to spam with requests.
        rpc_url: String,

        /// The number of txs to send per second.
        #[arg(short, long, default_value = "10", long_help = "Number of txs to send per second", visible_aliases = &["tps"])]
        intensity: Option<usize>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ContenderCli::parse();
    match args.command {
        ContenderSubcommand::Spam { rpc_url, intensity } => {
            spam_rpc(&rpc_url, intensity.unwrap_or_default())?;
        }
    }
    Ok(())
}
