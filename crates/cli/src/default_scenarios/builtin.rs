use super::fill_block::{fill_block, FillBlockArgs, FillBlockCliArgs};
use crate::{
    commands::common::SendSpamCliArgs,
    default_scenarios::{
        eth_functions::{EthFunctionsArgs, EthFunctionsCliArgs},
        storage::{StorageStressArgs, StorageStressCliArgs},
        transfers::{TransferStressArgs, TransferStressCliArgs},
    },
};
use clap::Subcommand;
use contender_core::{
    error::{ContenderError, RuntimeParamErrorKind},
    generator::types::AnyProvider,
};
use contender_testfile::TestConfig;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Subcommand)]
/// User-facing subcommands for builtin scenarios.
pub enum BuiltinScenarioCli {
    /// Fill blocks with simple gas-consuming transactions.
    FillBlock(FillBlockCliArgs),
    /// Spam specific opcodes & precompiles.
    EthFunctions(EthFunctionsCliArgs),
    /// Fill storage slots with random data.
    Storage(StorageStressCliArgs),
    /// Perform a large number of transfers. ETH is transferred to the sender if --recipient is not set.
    Transfers(TransferStressCliArgs),
}

#[derive(Clone, Debug)]
pub enum BuiltinScenario {
    FillBlock(FillBlockArgs),
    EthFunctions(EthFunctionsArgs),
    Storage(StorageStressArgs),
    Transfers(TransferStressArgs),
}

pub trait ToTestConfig {
    fn to_testconfig(&self) -> TestConfig;
}

impl BuiltinScenarioCli {
    pub async fn to_builtin_scenario(
        &self,
        provider: &AnyProvider,
        spam_args: &SendSpamCliArgs,
    ) -> Result<BuiltinScenario, ContenderError> {
        match self.to_owned() {
            BuiltinScenarioCli::FillBlock(args) => fill_block(provider, spam_args, &args).await,

            BuiltinScenarioCli::EthFunctions(args) => {
                let args: EthFunctionsArgs = args.into();
                if args.opcodes.is_empty() && args.precompiles.is_empty() {
                    return Err(ContenderError::InvalidRuntimeParams(
                        RuntimeParamErrorKind::MissingArgs(format!(
                            "{} or {}",
                            ansi_term::Style::new().bold().paint("--opcode (-o)"),
                            ansi_term::Style::new().bold().paint("--precompile (-p)"),
                        )),
                    ));
                }
                Ok(BuiltinScenario::EthFunctions(args))
            }

            BuiltinScenarioCli::Storage(args) => {
                let bad_args_err = |name: &str| {
                    ContenderError::InvalidRuntimeParams(RuntimeParamErrorKind::InvalidArgs(
                        format!(
                            "{} must be greater than 0",
                            ansi_term::Style::new().bold().paint(name)
                        ),
                    ))
                };
                if args.num_slots == 0 {
                    return Err(bad_args_err("--num-slots (-s)"));
                }
                if args.num_iterations == 0 {
                    return Err(bad_args_err("--num-iterations (-n)"));
                }

                Ok(BuiltinScenario::Storage(args.into()))
            }

            BuiltinScenarioCli::Transfers(args) => Ok(BuiltinScenario::Transfers(args.into())),
        }
    }
}

impl Display for BuiltinScenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use BuiltinScenario::*;
        match self {
            FillBlock(_) => {
                write!(f, "fill-block",)
            }
            EthFunctions(_) => {
                write!(f, "eth-functions",)
            }
            Storage(_) => {
                write!(f, "storage")
            }
            Transfers(_) => {
                write!(f, "transfers")
            }
        }
    }
}

impl From<BuiltinScenario> for TestConfig {
    fn from(scenario: BuiltinScenario) -> Self {
        use BuiltinScenario::*;
        let args = match scenario {
            FillBlock(args) => Box::new(args) as Box<dyn ToTestConfig>,
            EthFunctions(args) => Box::new(args),
            Storage(args) => Box::new(args),
            Transfers(args) => Box::new(args),
        };
        args.to_testconfig()
    }
}
