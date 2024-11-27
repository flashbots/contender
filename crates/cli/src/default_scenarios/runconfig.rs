use alloy::primitives::Address;
use contender_core::generator::types::{CreateDefinition, FunctionCallDefinition, SpamRequest};
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};

use super::bytecode::SPAM_ME;

#[derive(Serialize, Deserialize, Debug, Clone, clap::ValueEnum)]
pub enum BuiltinScenario {
    FillBlock,
}

pub enum BuiltinScenarioConfig {
    FillBlock {
        max_gas_per_block: u64,
        num_txs: u64,
        sender: Address,
    },
}

impl BuiltinScenarioConfig {
    pub fn fill_block(max_gas_per_block: u64, num_txs: u64, sender: Address) -> Self {
        Self::FillBlock {
            max_gas_per_block,
            num_txs,
            sender,
        }
    }
}

impl Into<TestConfig> for BuiltinScenarioConfig {
    fn into(self) -> TestConfig {
        match self {
            Self::FillBlock {
                max_gas_per_block,
                num_txs,
                sender,
            } => {
                let gas_per_tx = max_gas_per_block / num_txs;
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
                        bytecode: SPAM_ME.to_owned(),
                        from: sender.to_string(),
                    }]),
                    setup: None,
                    spam: Some(spam_txs),
                }
            }
        }
    }
}
