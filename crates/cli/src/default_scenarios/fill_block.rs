use crate::{
    commands::common::SendSpamCliArgs,
    default_scenarios::{builtin::ToTestConfig, contracts, BuiltinScenario},
};
use alloy::providers::Provider;
use clap::{arg, Parser};
use contender_core::{
    error::ContenderError,
    generator::{
        types::{AnyProvider, SpamRequest},
        CreateDefinition, FunctionCallDefinition,
    },
};
use contender_testfile::TestConfig;
use tracing::{info, warn};

#[derive(Parser, Clone, Debug)]
/// Taken from the CLI, this is used to fill a block with transactions.
pub struct FillBlockCliArgs {
    #[arg(short = 'g', long, long_help = "Override gas used per block. By default, the block limit is used.", visible_aliases = ["gas"])]
    pub max_gas_per_block: Option<u64>,
}

#[derive(Clone, Debug)]
/// Full arguments for the fill-block scenario.
pub struct FillBlockArgs {
    pub max_gas_per_block: u64,
    pub num_txs: u64,
}

pub async fn fill_block(
    provider: &AnyProvider,
    spam_args: &SendSpamCliArgs,
    args: &FillBlockCliArgs,
) -> Result<BuiltinScenario, ContenderError> {
    let SendSpamCliArgs {
        txs_per_block,
        txs_per_second,
        ..
    } = spam_args.to_owned();

    // determine gas limit
    let gas_limit = if let Some(max_gas) = args.max_gas_per_block {
        max_gas
    } else {
        let block_gas_limit = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .map_err(|e| ContenderError::with_err(e, "provider failed to get block"))?
            .map(|b| b.header.gas_limit);
        if block_gas_limit.is_none() {
            warn!("Could not get block gas limit from provider, using default 30M");
        }
        block_gas_limit.unwrap_or(30_000_000)
    };

    let num_txs = txs_per_block.unwrap_or(txs_per_second.unwrap_or_default());
    let gas_per_tx = gas_limit / num_txs;

    info!("Attempting to fill blocks with {gas_limit} gas; sending {num_txs} txs, each with gas limit {gas_per_tx}.");
    Ok(BuiltinScenario::FillBlock(FillBlockArgs {
        max_gas_per_block: gas_limit,
        num_txs,
    }))
}

impl ToTestConfig for FillBlockArgs {
    /// Convert the FillBlockArgs to a TestConfig.
    fn to_testconfig(&self) -> TestConfig {
        let FillBlockArgs {
            max_gas_per_block,
            num_txs,
        } = *self;
        let gas_per_tx = max_gas_per_block / num_txs;
        let spam_txs = (0..num_txs)
            .map(|_| {
                SpamRequest::Tx(
                    FunctionCallDefinition::new(contracts::SPAM_ME.template_name())
                        .with_signature("consumeGas()")
                        .with_from_pool("spammers")
                        .with_kind("fill-block")
                        .with_gas_limit(gas_per_tx),
                )
            })
            .collect::<Vec<_>>();

        TestConfig {
            env: None,
            create: Some(vec![CreateDefinition {
                contract: contracts::SPAM_ME.into(),
                from: None,
                from_pool: Some("admin".to_owned()),
            }]),
            setup: None,
            spam: Some(spam_txs),
        }
    }
}
