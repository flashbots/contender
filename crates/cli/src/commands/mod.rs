pub mod admin;
pub mod common;
mod contender_subcommand;
pub mod db;
mod report;
mod setup;
mod spam;
mod spamd;

use clap::Parser;

pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use report::report;
// pub use run::{run, RunCommandArgs};
pub use setup::{setup, SetupCliArgs, SetupCommandArgs};
pub use spam::{spam, EngineArgs, SpamCliArgs, SpamCommandArgs, SpamScenario};
pub use spamd::spamd;

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
