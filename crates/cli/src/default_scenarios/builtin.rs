use std::fmt::{Display, Formatter};

use clap::Subcommand;
use contender_core::generator::types::AnyProvider;
use contender_testfile::TestConfig;

use crate::commands::common::SendSpamCliArgs;

use super::fill_block::{fill_block, fill_block_config, FillBlockArgs, FillBlockCliArgs};

#[derive(Subcommand, Debug)]
/// User-facing subcommands for builtin scenarios.
pub enum BuiltinScenarioCli {
    /// Fill blocks with simple gas-consuming transactions.
    FillBlock(FillBlockCliArgs),
}

#[derive(Clone, Debug)]
pub enum BuiltinScenario {
    FillBlock(FillBlockArgs),
}

impl BuiltinScenarioCli {
    pub async fn to_builtin_scenario(
        &self,
        provider: &AnyProvider,
        spam_args: &SendSpamCliArgs,
    ) -> Result<BuiltinScenario, Box<dyn std::error::Error>> {
        match self {
            BuiltinScenarioCli::FillBlock(args) => fill_block(provider, spam_args, args).await,
        }
    }
}

impl Display for BuiltinScenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinScenario::FillBlock(_) => {
                write!(f, "fill-block",)
            }
        }
    }
}

impl From<BuiltinScenario> for TestConfig {
    fn from(scenario: BuiltinScenario) -> Self {
        match scenario {
            BuiltinScenario::FillBlock(args) => fill_block_config(args),
        }
    }
}
