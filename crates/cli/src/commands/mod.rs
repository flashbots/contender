mod report;
mod run;
mod setup;
mod spam;
mod types;

use clap::Parser;

pub use report::report;
pub use run::run;
pub use setup::setup;
pub use spam::spam;
pub use types::ContenderSubcommand;

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
