use std::fmt::Display;

use alloy::providers::Provider;
use contender_core::generator::types::{
    AnyProvider, CreateDefinition, FunctionCallDefinition, SpamRequest,
};
use contender_testfile::TestConfig;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

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
                fill_percent: _,
            } => write!(f, "fill-block"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BuiltinScenarioConfig {
    FillBlock {
        max_gas_per_block: u64,
        num_txs: u64,
        fill_percent: u32,
    },
}

impl BuiltinScenarioConfig {
    pub async fn fill_block(
        provider: &AnyProvider,
        num_txs: u64,
        fill_percent: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let block = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await?;
        let limit = if let Some(block) = block {
            block.header.gas_limit
        } else {
            warn!("Could not get block gas limit, using default 30M");
            30_000_000
        };
        Ok(Self::FillBlock {
            max_gas_per_block: limit,
            num_txs,
            fill_percent,
        })
    }
}

impl From<BuiltinScenarioConfig> for TestConfig {
    fn from(scenario: BuiltinScenarioConfig) -> Self {
        match scenario {
            BuiltinScenarioConfig::FillBlock {
                max_gas_per_block,
                num_txs,
                fill_percent,
            } => {
                let gas_per_tx = (fill_percent as u64 * max_gas_per_block) / (num_txs * 100);
                info!("Filling blocks to {fill_percent}% ({}/{max_gas_per_block}); sending {num_txs} txs with gas limit {gas_per_tx}", fill_percent as u64 * max_gas_per_block / 100);
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
                        name: "SpamMe".to_owned(),
                        bytecode: bytecode::SPAM_ME.to_owned(),
                        from: None,
                        from_pool: Some("spammers".to_owned()),
                    }]),
                    setup: None,
                    spam: Some(spam_txs),
                }
            }
        }
    }
}
