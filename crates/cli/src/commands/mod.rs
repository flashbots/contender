pub mod admin;
pub mod campaign;
pub mod common;
mod contender_subcommand;
pub mod db;
pub mod error;
pub mod replay;
mod setup;
mod spam;
mod spamd;

use clap::Parser;
pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use setup::{setup, SetupCommandArgs};
pub use spam::{spam, EngineArgs, SpamCampaignContext, SpamCliArgs, SpamCommandArgs, SpamScenario};
pub use spamd::spamd;

use crate::error::CliError;

pub type Result<T> = std::result::Result<T, CliError>;

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
