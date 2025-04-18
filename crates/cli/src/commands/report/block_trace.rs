use alloy::network::{AnyRpcBlock, AnyTransactionReceipt};
use alloy::providers::ext::DebugApi;
use alloy::{
    providers::Provider,
    rpc::types::trace::geth::{
        GethDebugBuiltInTracerType, GethDebugTracerConfig, GethDebugTracerType,
        GethDebugTracingOptions, GethDefaultTracingOptions, GethTrace,
    },
};
use contender_core::db::RunTx;
use contender_core::error::ContenderError;
use contender_core::generator::types::AnyProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TxTraceReceipt {
    pub trace: GethTrace,
    pub receipt: AnyTransactionReceipt,
}

impl TxTraceReceipt {
    pub fn new(trace: GethTrace, receipt: AnyTransactionReceipt) -> Self {
        Self { trace, receipt }
    }
}

pub async fn get_block_data(
    txs: &[RunTx],
    rpc_client: &AnyProvider,
) -> Result<Vec<AnyRpcBlock>, Box<dyn std::error::Error>> {
    // filter out txs with no block number
    let txs: Vec<RunTx> = txs
        .iter()
        .filter(|tx| tx.block_number.is_some())
        .cloned()
        .collect();

    // find block range of txs
    let (min_block, max_block) = txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
        let bn = tx.block_number.expect("tx has no block number");
        (min.min(bn), max.max(bn))
    });

    // pad block range on each side
    let block_padding = 3;
    let min_block = min_block - block_padding;
    let max_block = max_block + block_padding;

    let rpc_client = Arc::new(rpc_client.clone());

    // get block data
    let mut all_blocks: Vec<AnyRpcBlock> = vec![];
    let (sender, mut receiver) =
        tokio::sync::mpsc::channel::<AnyRpcBlock>((max_block - min_block) as usize + 1);

    let mut handles = vec![];
    for block_num in min_block..=max_block {
        let rpc_client = rpc_client.clone();
        let sender = sender.clone();
        let handle = tokio::task::spawn(async move {
            println!("getting block {}...", block_num);
            let block = rpc_client
                .get_block_by_number(block_num.into())
                .full()
                .await
                .expect("failed to get block");
            if let Some(block) = block {
                println!("read block {}", block.header.number);
                sender.send(block).await.expect("failed to cache block");
            }
        });
        handles.push(handle);
    }
    futures::future::join_all(handles).await;
    receiver.close();

    while let Some(res) = receiver.recv().await {
        all_blocks.push(res);
    }

    Ok(all_blocks)
}

pub async fn get_block_traces(
    full_blocks: &[AnyRpcBlock],
    rpc_client: &AnyProvider,
) -> Result<Vec<TxTraceReceipt>, Box<dyn std::error::Error>> {
    // get tx traces for all txs in all_blocks
    let mut all_traces = vec![];
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<TxTraceReceipt>(
        full_blocks.iter().map(|b| b.transactions.len()).sum(),
    );

    for block in full_blocks {
        let mut tx_tasks = vec![];
        for tx_hash in block.transactions.hashes() {
            let rpc_client = rpc_client.clone();
            let sender = sender.clone();
            let task = tokio::task::spawn(async move {
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
                    }).unwrap();

                // receipt might fail if we target a non-ETH chain
                // so if it does fail, we just ignore it
                let receipt = rpc_client.get_transaction_receipt(tx_hash).await;
                if let Ok(receipt) = receipt {
                    if let Some(receipt) = receipt {
                        println!("got receipt for tx {:?}", tx_hash);
                        sender
                            .send(TxTraceReceipt::new(trace, receipt))
                            .await
                            .unwrap();
                    } else {
                        println!("no receipt for tx {:?}", tx_hash);
                    }
                } else {
                    println!("ignored receipt for tx {:?} (failed to decode)", tx_hash);
                }
            });
            tx_tasks.push(task);
        }
        println!("waiting for traces from block {}...", block.header.number);
        futures::future::join_all(tx_tasks).await;
        println!("finished tracing block {}", block.header.number);
    }

    receiver.close();

    while let Some(res) = receiver.recv().await {
        println!("received trace for {}", res.receipt.transaction_hash);
        all_traces.push(res);
    }

    Ok(all_traces)
}
