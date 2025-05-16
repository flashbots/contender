use super::{bytecode, BuiltinScenario};
use crate::commands::common::SendSpamCliArgs;
use alloy::providers::Provider;
use clap::{arg, Parser};
use contender_core::generator::types::{
    AnyProvider, CreateDefinition, FunctionCallDefinition, SpamRequest,
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
) -> Result<BuiltinScenario, Box<dyn std::error::Error>> {
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
            .await?
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

pub fn fill_block_config(args: FillBlockArgs) -> TestConfig {
    let FillBlockArgs {
        max_gas_per_block,
        num_txs,
    } = args;
    let gas_per_tx = max_gas_per_block / num_txs;
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
