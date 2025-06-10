use super::fill_block::{fill_block, fill_block_config, FillBlockArgs, FillBlockCliArgs};
use crate::{
    commands::common::SendSpamCliArgs,
    default_scenarios::eth_functions::{
        eth_functions_config, EthFunctionsArgs, EthFunctionsCliArgs,
    },
};
use clap::Subcommand;
use contender_core::generator::types::AnyProvider;
use contender_testfile::TestConfig;
use std::fmt::{Display, Formatter};

#[derive(Subcommand, Debug)]
/// User-facing subcommands for builtin scenarios.
pub enum BuiltinScenarioCli {
    /// Fill blocks with simple gas-consuming transactions.
    FillBlock(FillBlockCliArgs),
    /// Spam specific opcodes & precompiles.
    EthFunctions(EthFunctionsCliArgs),
}

#[derive(Clone, Debug)]
pub enum BuiltinScenario {
    FillBlock(FillBlockArgs),
    EthFunctions(EthFunctionsArgs),
}

impl BuiltinScenarioCli {
    pub async fn to_builtin_scenario(
        &self,
        provider: &AnyProvider,
        spam_args: &SendSpamCliArgs,
    ) -> Result<BuiltinScenario, Box<dyn std::error::Error>> {
        match self {
            BuiltinScenarioCli::FillBlock(args) => fill_block(provider, spam_args, args).await,
            BuiltinScenarioCli::EthFunctions(args) => {
                Ok(BuiltinScenario::EthFunctions(args.into()))
            }
        }
    }
}

impl Display for BuiltinScenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinScenario::FillBlock(_) => {
                write!(f, "fill-block",)
            }
            BuiltinScenario::EthFunctions(_) => {
                write!(f, "eth-functions",)
            }
        }
    }
}

impl From<BuiltinScenario> for TestConfig {
    fn from(scenario: BuiltinScenario) -> Self {
        match scenario {
            BuiltinScenario::FillBlock(args) => fill_block_config(args),
            BuiltinScenario::EthFunctions(args) => eth_functions_config(args),
        }
    }
}
