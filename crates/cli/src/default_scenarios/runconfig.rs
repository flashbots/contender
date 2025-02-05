use std::fmt::Display;

use alloy::primitives::Address;
use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition, SpamRequest};
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};

use super::bytecode;

#[derive(Serialize, Deserialize, Debug, Clone, clap::ValueEnum)]
pub enum BuiltinScenario {
    FillBlock,
}

impl Display for BuiltinScenarioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinScenarioConfig::FillBlock {
                max_gas_per_block: _,
                num_txs: _,
                sender: _,
                fill_percent: _,
            } => write!(f, "fill-block"),
        }
    }
}

pub enum BuiltinScenarioConfig {
    FillBlock {
        max_gas_per_block: u128,
        num_txs: u64,
        sender: Address,
        fill_percent: u16,
    },
}

impl BuiltinScenarioConfig {
    pub fn fill_block(
        max_gas_per_block: u128,
        num_txs: u64,
        sender: Address,
        fill_percent: u16,
    ) -> Self {
        Self::FillBlock {
            max_gas_per_block,
            num_txs,
            sender,
            fill_percent,
        }
    }
}

impl From<BuiltinScenarioConfig> for TestConfig {
    fn from(scenario: BuiltinScenarioConfig) -> Self {
        match scenario {
            BuiltinScenarioConfig::FillBlock {
                max_gas_per_block,
                num_txs,
                sender,
                fill_percent,
            } => {
                let gas_per_tx =
                    ((max_gas_per_block / num_txs as u128) / 100) * fill_percent as u128;
                println!(
                    "Filling blocks to {}% with {} gas per tx",
                    fill_percent, gas_per_tx
                );
                let spam_txs = (0..num_txs)
                    .map(|_| {
                        SpamRequest::Tx(FunctionCallDefinition {
                            to: "{SpamMe}".to_owned(),
                            from: Some(sender.to_string()),
                            signature: "consumeGas(uint256 gas)".to_owned(),
                            from_pool: None,
                            args: Some(vec![gas_per_tx.to_string()]),
                            value: None,
                            fuzz: None,
                            kind: Some("fill-block".to_owned()),
                        })
                    })
                    .collect::<Vec<_>>();

                TestConfig {
                    env: None,
                    create: Some(vec![CreateDefinition {
                        name: "SpamMe".to_owned(),
                        bytecode: bytecode::SPAM_ME.to_owned(),
                        from: Some(sender.to_string()),
                        from_pool: None,
                    }]),
                    setup: None,
                    spam: Some(spam_txs),
                }
            }
        }
    }
}
