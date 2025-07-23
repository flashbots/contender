use std::ops::Deref;

use super::fill_block::{fill_block, FillBlockArgs, FillBlockCliArgs};
use crate::{
    commands::SpamCliArgs,
    default_scenarios::{
        blobs::BlobsCliArgs,
        custom_contract::{CustomContractArgs, CustomContractCliArgs},
        erc20::{Erc20Args, Erc20CliArgs},
        eth_functions::{opcodes::EthereumOpcode, EthFunctionsArgs, EthFunctionsCliArgs},
        revert::RevertCliArgs,
        storage::{StorageStressArgs, StorageStressCliArgs},
        stress::StressCliArgs,
        transfers::{TransferStressArgs, TransferStressCliArgs},
        uni_v2::{UniV2Args, UniV2CliArgs},
    },
    util::load_seedfile,
};
use alloy::primitives::U256;
use clap::Subcommand;
use contender_core::{
    agent_controller::AgentStore,
    error::{ContenderError, RuntimeParamErrorKind},
    generator::{types::AnyProvider, RandSeed},
};
use contender_testfile::TestConfig;
use strum::IntoEnumIterator;
use tracing::warn;

#[derive(Clone, Debug, Subcommand)]
pub enum BuiltinScenarioCli {
    /// Send EIP-4844 blob transactions.
    Blobs(BlobsCliArgs),
    /// Deploy and spam a custom contract.
    Contract(CustomContractCliArgs),
    /// Spam specific opcodes & precompiles.
    EthFunctions(EthFunctionsCliArgs),
    /// Transfer ERC20 tokens.
    Erc20(Erc20CliArgs),
    /// Fill blocks with simple gas-consuming transactions.
    FillBlock(FillBlockCliArgs),
    /// Send reverting transactions.
    Revert(RevertCliArgs),
    /// Fill storage slots with random data.
    Storage(StorageStressCliArgs),
    /// Run a comprehensive stress test with various parameters.
    Stress(StressCliArgs),
    /// Simple ETH transfers. ETH is transferred to the sender if --recipient is not set.
    Transfers(TransferStressCliArgs),
    /// Send swaps on UniV2 with custom tokens.
    UniV2(UniV2CliArgs),
}

#[derive(Clone, Debug)]
pub enum BuiltinScenario {
    Blobs(BlobsCliArgs),
    Contract(CustomContractArgs),
    Erc20(Erc20Args),
    EthFunctions(EthFunctionsArgs),
    FillBlock(FillBlockArgs),
    Revert(RevertCliArgs),
    Storage(StorageStressArgs),
    Transfers(TransferStressArgs),
    Stress(StressCliArgs),
    UniV2(UniV2Args),
}

pub trait ToTestConfig {
    fn to_testconfig(&self) -> TestConfig;
}

impl BuiltinScenarioCli {
    pub async fn to_builtin_scenario(
        &self,
        provider: &AnyProvider,
        spam_args: &SpamCliArgs,
    ) -> Result<BuiltinScenario, ContenderError> {
        match self.to_owned() {
            BuiltinScenarioCli::Blobs(args) => Ok(BuiltinScenario::Blobs(args)),

            BuiltinScenarioCli::Contract(args) => Ok(BuiltinScenario::Contract(
                CustomContractArgs::from_cli_args(args)?,
            )),

            BuiltinScenarioCli::Erc20(args) => {
                let seed = spam_args.eth_json_rpc_args.seed.to_owned().unwrap_or(
                    load_seedfile().map_err(|e| {
                        ContenderError::with_err(e.deref(), "failed to load seedfile")
                    })?,
                );
                let seed = RandSeed::seed_from_str(&seed);
                let mut agents = AgentStore::new();
                agents.init(
                    &["spammers"],
                    spam_args.spam_args.accounts_per_agent as usize,
                    &seed,
                );
                let spammers = agents.get_agent("spammers");
                if spammers.is_none() {
                    return Err(ContenderError::GenericError(
                        "spammers is not present in the agent store",
                        String::new(),
                    ));
                }
                let spammers = spammers.expect("spammers");

                Ok(BuiltinScenario::Erc20(Erc20Args::from_cli_args(
                    args,
                    &spammers.all_addresses(),
                )))
            }

            BuiltinScenarioCli::FillBlock(args) => {
                fill_block(provider, &spam_args.spam_args, &args).await
            }

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

            BuiltinScenarioCli::Revert(args) => {
                if args.gas_use < 21000 {
                    warn!("gas limit is less than 21000. Your transactions will consume more gas than this.");
                }
                Ok(BuiltinScenario::Revert(args))
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

            BuiltinScenarioCli::Stress(args) => {
                if args.disable_storage
                    && args.disable_transfers
                    && args.disable_all_opcodes
                    && args.disable_all_precompiles
                {
                    return Err::<_, ContenderError>(
                        RuntimeParamErrorKind::MissingArgs(
                            "At least one stress test must be enabled".to_string(),
                        )
                        .into(),
                    );
                }
                Ok(BuiltinScenario::Stress(args))
            }

            BuiltinScenarioCli::UniV2(args) => {
                let check_zero = |name: &str, value: U256| {
                    if value == U256::ZERO {
                        return Err::<_, ContenderError>(
                            RuntimeParamErrorKind::InvalidArgs(format!(
                                "{} must be greater than 0",
                                ansi_term::Style::new().bold().paint(name),
                            ))
                            .into(),
                        );
                    }
                    Ok(())
                };
                check_zero("--initial-token-supply (-i)", args.initial_token_supply)?;
                check_zero("--num-tokens (-n)", U256::from(args.num_tokens))?;
                check_zero("--weth-per-token (-w)", args.weth_per_token)?;
                if let Some(amount) = args.token_trade_amount {
                    check_zero("--token-trade-amount", amount)?;
                }
                Ok(BuiltinScenario::UniV2(args.into()))
            }
        }
    }
}

impl BuiltinScenario {
    pub fn title(&self) -> String {
        use BuiltinScenario::*;
        match self {
            Blobs(_) => "blobs".to_string(),
            Contract(args) => format!("custom contract: {}", args.contract.name.to_owned()),
            Erc20(_) => "erc20 transfers".to_string(),
            FillBlock(_) => "fill-block".to_string(),
            EthFunctions(args) => args
                .opcodes
                .iter()
                .map(|opcode| opcode.to_string().to_lowercase())
                .chain(
                    args.precompiles
                        .iter()
                        .map(|p| p.to_string().to_lowercase()),
                )
                .collect::<Vec<_>>()
                .join(", "),
            Revert(_) => "reverts".to_owned(),
            Storage(args) => {
                let iters_str = if args.num_iterations > 1 {
                    format!(", {} iterations", args.num_iterations)
                } else {
                    String::new()
                };
                format!("storage ({} slots{iters_str})", args.num_slots)
            }
            Transfers(_) => "ETH transfers".to_string(),
            Stress(args) => {
                let mut disabled = vec![];
                if args.disable_storage {
                    disabled.push("storage");
                }
                if args.disable_transfers {
                    disabled.push("transfers");
                }
                if args.disable_all_opcodes {
                    disabled.push("all opcodes");
                }
                if args.disable_all_precompiles {
                    disabled.push("all precompiles");
                }
                let disabled_str = if !disabled.is_empty() {
                    format!(" (sans {})", disabled.join(", "))
                } else {
                    String::new()
                };

                format!(
                    "stress{disabled_str}: {} opcodes, {} storage slots",
                    EthereumOpcode::iter().len()
                        - args
                            .disable_opcodes
                            .as_ref()
                            .map(|oc| oc.len())
                            .unwrap_or(0),
                    args.storage.num_slots
                )
            }
            UniV2(args) => {
                format!("uni-v2 ({} tokens)", args.num_tokens)
            }
        }
    }
}

impl From<BuiltinScenario> for TestConfig {
    fn from(scenario: BuiltinScenario) -> Self {
        use BuiltinScenario::*;
        let args = match scenario {
            // TODO: can we use a macro to DRY this out?
            Blobs(args) => Box::new(args) as Box<dyn ToTestConfig>,
            Contract(args) => Box::new(args),
            Erc20(args) => Box::new(args),
            FillBlock(args) => Box::new(args),
            EthFunctions(args) => Box::new(args),
            Revert(args) => Box::new(args),
            Storage(args) => Box::new(args),
            Transfers(args) => Box::new(args),
            Stress(args) => Box::new(args),
            UniV2(args) => Box::new(args),
        };
        args.to_testconfig()
    }
}
