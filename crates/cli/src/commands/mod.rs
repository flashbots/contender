mod contender_subcommand;
pub mod db;
pub mod report;
pub mod run;
pub mod setup;
pub mod spam;

use clap::Parser;

pub use contender_subcommand::{ContenderSubcommand, DbCommand};
pub use report::report;
pub use run::run;
pub use setup::setup;
pub use spam::spam;

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
