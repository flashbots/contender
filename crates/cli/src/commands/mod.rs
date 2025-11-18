pub mod admin;
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
pub use spam::{spam, EngineArgs, SpamCliArgs, SpamCommandArgs, SpamScenario};
pub use spamd::spamd;

use crate::error::ContenderError;

pub type Result<T> = std::result::Result<T, ContenderError>;

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
