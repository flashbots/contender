mod contender_subcommand;
mod report;
mod run;
mod setup;
mod spam;

use clap::Parser;

pub use contender_subcommand::ContenderSubcommand;
pub use report::report;
pub use run::run;
pub use setup::setup;
pub use spam::{spam, SpamCommandArgs};

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
