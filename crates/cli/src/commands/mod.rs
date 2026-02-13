pub mod admin;
pub mod campaign;
pub mod common;
mod contender_subcommand;
pub mod db;
pub mod error;
pub mod replay;
mod setup;
mod spam;

use clap::{Parser, ValueEnum};
pub use contender_subcommand::*;
pub use setup::*;
pub use spam::*;
use std::path::PathBuf;

use crate::error::CliError;

pub type Result<T> = std::result::Result<T, CliError>;

/// Output format for reports.
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ReportFormat {
    #[default]
    Html,
    Json,
}

#[derive(Parser, Debug)]
#[command(
    name = "contender",
    version,
    author = "Flashbots",
    about = "A flexible JSON-RPC spammer for EVM chains."
)]
pub struct ContenderCli {
    /// Override the default data directory (~/.contender).
    /// This directory stores the database and reports.
    #[arg(
        long,
        global = true,
        env = "CONTENDER_DATA_DIR",
        value_name = "PATH"
    )]
    pub data_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: ContenderSubcommand,
}

impl ContenderCli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
