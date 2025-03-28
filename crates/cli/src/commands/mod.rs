pub mod common;
mod contender_subcommand;
mod db;
mod report;
mod run;
mod setup;
mod spam;

use clap::Parser;

pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use db::*;
pub use report::report;
pub use run::{run, RunCommandArgs};
pub use setup::{setup, SetupCliArgs};
pub use spam::{spam, SpamCliArgs, SpamCommandArgs};

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
