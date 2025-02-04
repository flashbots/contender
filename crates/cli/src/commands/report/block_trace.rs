use crate::commands::report::cache::CacheFile;
use alloy::providers::ext::DebugApi;
use alloy::rpc::types::Block;
use alloy::{
    providers::Provider,
    rpc::types::{
        trace::geth::{
            GethDebugBuiltInTracerType, GethDebugTracerConfig, GethDebugTracerType,
            GethDebugTracingOptions, GethDefaultTracingOptions, GethTrace,
        },
        TransactionReceipt,
    },
};

use contender_core::error::ContenderError;
use contender_core::{db::RunTx, generator::types::EthProvider};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TxTraceReceipt {
    pub trace: GethTrace,
    pub receipt: TransactionReceipt,
}

impl TxTraceReceipt {
    pub fn new(trace: GethTrace, receipt: TransactionReceipt) -> Self {
        Self { trace, receipt }
    }
}

pub async fn get_block_trace_data(
    txs: &[RunTx],
    rpc_client: &EthProvider,
) -> Result<(Vec<TxTraceReceipt>, Vec<Block>), Box<dyn std::error::Error>> {
    if std::env::var("DEBUG_USEFILE").is_ok() {
        println!("DEBUG_USEFILE detected: using cached data");
        // load trace data from file
        let cache_data = CacheFile::load()?;
        return Ok((cache_data.traces, cache_data.blocks));
    }

    // find block range of txs
    let (min_block, max_block) = txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
        (min.min(tx.block_number), max.max(tx.block_number))
    });

    // pad block range on each side
    let block_padding = 3;
    let min_block = min_block - block_padding;
    let max_block = max_block + block_padding;

    // get block data
    let mut all_blocks = vec![];
    for block_num in min_block..=max_block {
        let block = rpc_client
            .get_block_by_number(block_num.into(), true)
            .await?;
        if let Some(block) = block {
            println!("read block {}", block.header.number);
            all_blocks.push(block);
        }
    }

    // get tx traces for all txs in all_blocks
    let mut all_traces = vec![];
    for block in &all_blocks {
        for tx_hash in block.transactions.hashes() {
            println!("tracing tx {:?}", tx_hash);
            let trace = rpc_client
                .debug_trace_transaction(
                    tx_hash,
                    GethDebugTracingOptions {
                        config: GethDefaultTracingOptions::default(),
                        tracer: Some(GethDebugTracerType::BuiltInTracer(
                            GethDebugBuiltInTracerType::PreStateTracer,
                        )),
                        tracer_config: GethDebugTracerConfig::default(),
                        timeout: None,
                    },
                )
                .await
                .map_err(|e| {
                    ContenderError::with_err(
                        e,
                        "debug_traceTransaction failed. Make sure geth-style tracing is enabled on your node.",
                    )
                })?;

            // receipt might fail if we target a non-ETH chain
            // so if it does fail, we just ignore it
            let receipt = rpc_client.get_transaction_receipt(tx_hash).await;
            if let Ok(receipt) = receipt {
                if let Some(receipt) = receipt {
                    println!("got receipt for tx {:?}", tx_hash);
                    all_traces.push(TxTraceReceipt::new(trace, receipt));
                } else {
                    println!("no receipt for tx {:?}", tx_hash);
                }
            } else {
                println!("ignored receipt for tx {:?} (failed to decode)", tx_hash);
            }
        }
    }

    Ok((all_traces, all_blocks))
}
