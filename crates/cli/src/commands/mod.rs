pub mod admin;
pub mod campaign;
pub mod common;
mod contender_subcommand;
pub mod db;
pub mod error;
pub mod replay;
mod setup;
mod spam;

use clap::Parser;
pub use contender_subcommand::*;
pub use setup::*;
pub use spam::*;

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
