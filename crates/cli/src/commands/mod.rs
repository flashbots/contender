pub mod common;
mod contender_subcommand;
pub mod db;
mod report;
mod run;
mod setup;
mod spam;
mod spamd;

use clap::Parser;

pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use report::report;
pub use run::{run, RunCommandArgs};
pub use setup::{setup, SetupCliArgs, SetupCommandArgs};
pub use spam::{spam, EngineArgs, SpamCliArgs, SpamCommandArgs};
pub use spamd::spamd;

#[derive(Parser, Debug)]
pub struct ContenderCli {
    #[command(subcommand)]
    pub command: ContenderSubcommand,

    #[arg(long = "optimism", long_help = "Set this flag when targeting an OP node.", visible_aliases = &["op"])]
    pub use_op: bool,
}

impl ContenderCli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
