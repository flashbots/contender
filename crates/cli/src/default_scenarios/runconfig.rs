use std::fmt::{Display, Formatter};

use alloy::providers::Provider;
use clap::Subcommand;
use contender_core::generator::types::{
    AnyProvider, CreateDefinition, FunctionCallDefinition, SpamRequest,
};
use contender_testfile::TestConfig;
use tracing::{info, warn};

use crate::commands::common::SendSpamCliArgs;

use super::{
    bytecode,
    fill_block::{FillBlockArgs, FillBlockCliArgs},
};

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
        let SendSpamCliArgs {
            txs_per_block,
            txs_per_second,
            ..
        } = spam_args;
        match self {
            BuiltinScenarioCli::FillBlock(args) => {
                let limit = if let Some(max_gas_per_block) = args.max_gas_per_block {
                    max_gas_per_block
                } else {
                    let block_gas_limit = provider
                        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
                        .await?
                        .map(|b| b.header.gas_limit);
                    if block_gas_limit.is_none() {
                        warn!("Could not get block gas limit from provider, using default 30M");
                    }
                    block_gas_limit.unwrap_or(30_000_000)
                };
                Ok(BuiltinScenario::FillBlock(FillBlockArgs {
                    max_gas_per_block: limit,
                    num_txs: txs_per_block.unwrap_or(txs_per_second.unwrap_or_default()),
                }))
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
        }
    }
}

impl From<BuiltinScenario> for TestConfig {
    fn from(scenario: BuiltinScenario) -> Self {
        match scenario {
            BuiltinScenario::FillBlock(args) => {
                let FillBlockArgs {
                    max_gas_per_block,
                    num_txs,
                } = args;
                let gas_per_tx = max_gas_per_block / num_txs;
                info!("Attempting to fill blocks with {max_gas_per_block} gas; sending {num_txs} txs, each with gas limit {gas_per_tx}.");
                let spam_txs = (0..num_txs)
                    .map(|_| {
                        SpamRequest::Tx(FunctionCallDefinition {
                            to: "{SpamMe5}".to_owned(),
                            from: None,
                            signature: "consumeGas()".to_owned(),
                            from_pool: Some("spammers".to_owned()),
                            args: None,
                            value: None,
                            fuzz: None,
                            kind: Some("fill-block".to_owned()),
                            gas_limit: Some(gas_per_tx),
                        })
                    })
                    .collect::<Vec<_>>();

                TestConfig {
                    env: None,
                    create: Some(vec![CreateDefinition {
                        name: "SpamMe5".to_owned(),
                        bytecode: bytecode::SPAM_ME.to_owned(),
                        from: None,
                        from_pool: Some("admin".to_owned()),
                    }]),
                    setup: None,
                    spam: Some(spam_txs),
                }
            }
        }
    }
}
