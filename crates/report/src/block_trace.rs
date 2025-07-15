use alloy::eips::BlockId;
use alloy::network::{AnyRpcBlock, AnyTransactionReceipt};
use alloy::providers::ext::DebugApi;
use alloy::{
    providers::Provider,
    rpc::types::trace::geth::{
        GethDebugBuiltInTracerType, GethDebugTracerConfig, GethDebugTracerType,
        GethDebugTracingOptions, GethDefaultTracingOptions, GethTrace,
    },
};
use contender_core::db::{DbOps, RunTx, SpamDuration};
use contender_core::error::ContenderError;
use contender_core::generator::types::AnyProvider;
use contender_core::util::get_block_time;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

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

pub async fn estimate_block_data(
    start_run_id: u64,
    end_run_id: u64,
    rpc_client: &AnyProvider,
    db: &(impl DbOps + Clone + Send + Sync + 'static),
) -> Result<Vec<AnyRpcBlock>, Box<dyn std::error::Error>> {
    let start_run = db.get_run(start_run_id)?.expect("run does not exist");
    let end_run = db.get_run(end_run_id)?.expect("run does not exist");
    let start_timestamp = (start_run.timestamp / 1000) as u64; // convert to seconds
    let end_timestamp = (end_run.timestamp / 1000) as u64; // convert to seconds
    let block_time = get_block_time(rpc_client).await?;

    // calculate the number of seconds to add based on the duration type
    let add_seconds = match end_run.duration {
        SpamDuration::Seconds(secs) => secs,
        SpamDuration::Blocks(blocks) => blocks * block_time,
    };

    let recent_block = rpc_client
        .get_block(BlockId::latest())
        .await?
        .expect("latest block not found");

    let start_time_delta = recent_block.header.timestamp - start_timestamp;
    let start_block_delta = start_time_delta / block_time;
    let start_block = recent_block.header.number.saturating_sub(start_block_delta);
    let end_time_delta = end_timestamp - start_timestamp + add_seconds;
    let end_block = start_block + (end_time_delta / block_time);

    get_blocks(start_block, end_block, rpc_client).await
}

async fn get_blocks(
    min_block: u64,
    max_block: u64,
    rpc_client: &AnyProvider,
) -> Result<Vec<AnyRpcBlock>, Box<dyn std::error::Error>> {
    // get block data
    let mut all_blocks: Vec<AnyRpcBlock> = vec![];
    let (sender, mut receiver) =
        tokio::sync::mpsc::channel::<AnyRpcBlock>((max_block - min_block) as usize + 1);

    let mut handles = vec![];
    for block_num in min_block..=max_block {
        let rpc_client = rpc_client.clone();
        let sender = sender.clone();
        let handle = tokio::task::spawn(async move {
            info!("getting block {block_num}...");
            let block = rpc_client
                .get_block_by_number(block_num.into())
                .full()
                .await
                .expect("failed to get block");
            if let Some(block) = block {
                debug!("read block {}", block.header.number);
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

    if txs.is_empty() {
        warn!("No landed transactions found. No block data is available.");
        return Ok(vec![]);
    }

    // find block range of txs
    let (min_block, max_block) = txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
        let bn = tx.block_number.expect("tx has no block number");
        (min.min(bn), max.max(bn))
    });

    // pad block range on each side
    let block_padding = 3;
    let min_block = min_block.saturating_sub(block_padding);
    let max_block = max_block.saturating_add(block_padding);

    let rpc_client = Arc::new(rpc_client.clone());

    get_blocks(min_block, max_block, &rpc_client).await
}

pub async fn get_block_traces(
    full_blocks: &[AnyRpcBlock],
    rpc_client: &AnyProvider,
) -> Result<Vec<TxTraceReceipt>, Box<dyn std::error::Error>> {
    // get tx traces for all txs in all_blocks
    let mut all_traces = vec![];
    if full_blocks.is_empty() {
        return Ok(all_traces);
    }
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<TxTraceReceipt>(
        full_blocks.iter().map(|b| b.transactions.len()).sum(),
    );

    for block in full_blocks {
        let mut tx_tasks = vec![];
        for tx_hash in block.transactions.hashes() {
            let rpc_client = rpc_client.clone();
            let sender = sender.clone();
            let task = tokio::task::spawn(async move {
                debug!("tracing tx {tx_hash:?}");
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
                        info!("got receipt for tx {tx_hash:?}");
                        sender
                            .send(TxTraceReceipt::new(trace, receipt))
                            .await
                            .map_err(|join_err| {
                                ContenderError::with_err(join_err, "failed to join trace receipt")
                            })?;
                    } else {
                        warn!("no receipt for tx {tx_hash:?}");
                    }
                } else {
                    warn!("ignored receipt for tx {tx_hash:?} (failed to decode)");
                }

                Ok::<_, ContenderError>(())
            });
            tx_tasks.push(task);
        }
        info!("waiting for traces from block {}...", block.header.number);
        futures::future::join_all(tx_tasks).await;
        info!("finished tracing block {}", block.header.number);
    }

    receiver.close();

    while let Some(res) = receiver.recv().await {
        debug!("received trace for {}", res.receipt.transaction_hash);
        all_traces.push(res);
    }

    Ok(all_traces)
}
