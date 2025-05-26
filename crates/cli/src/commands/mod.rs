pub mod admin;
pub mod common;
pub mod composite;
mod contender_subcommand;
mod composefile;
pub mod db;
mod report;
mod setup;
mod spam;
mod spamd;

use clap::Parser;

pub use composite::composite;
pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use report::report;
pub use setup::{setup, SetupCliArgs, SetupCommandArgs};
pub use spam::{spam, EngineArgs, SpamCliArgs, SpamCommandArgs, SpamScenario};
pub use spamd::spamd;

#[derive(Parser, Debug)]
#[command(
    name = "contender",
    version,
    author = "Flashbots",
    about = "A flexible JSON-RPC spammer for EVM chains."
)]
pub struct ContenderCli {
    #[command(subcommand)]
    pub command: ContenderSubcommand,
}

impl ContenderCli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
