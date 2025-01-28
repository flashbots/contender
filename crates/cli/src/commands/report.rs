use std::str::FromStr;

use alloy::{
    providers::{Provider, ProviderBuilder},
    rpc::types::{
        trace::geth::{
            GethDebugBuiltInTracerType, GethDebugTracerConfig, GethDebugTracerType,
            GethDebugTracingOptions, GethDefaultTracingOptions, GethTrace,
        },
        TransactionReceipt,
    },
    transports::http::reqwest::Url,
};
use csv::WriterBuilder;

use alloy::providers::ext::DebugApi;
use contender_core::{
    db::{DbOps, RunTx},
    generator::types::EthProvider,
};

use crate::util::write_run_txs;

pub async fn report(
    last_run_id: Option<u64>,
    preceding_runs: u64,
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    rpc_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_runs = db.num_runs()?;
    if num_runs == 0 {
        println!("No runs found in the database. Exiting.");
        return Ok(());
    }

    // if id is provided, check if it's valid
    let end_run_id = if let Some(id) = last_run_id {
        if id == 0 || id > num_runs {
            // panic!("Invalid run ID: {}", id);
            return Err(format!("Invalid run ID: {}", id).into());
        }
        id
    } else {
        // get latest run
        println!("No run ID provided. Using latest run ID: {}", num_runs);
        num_runs
    };

    // collect CSV report for each run_id
    let start_run_id = end_run_id - preceding_runs;
    let mut all_txs = vec![];
    for id in start_run_id..=end_run_id {
        let txs = db.get_run_txs(id)?;
        all_txs.extend_from_slice(&txs);
        save_csv_report(id, &txs)?;
    }

    let url = Url::from_str(rpc_url).expect("Invalid URL");
    let rpc_client = ProviderBuilder::new().on_http(url);

    // make the high-level report
    make_report(&all_txs, &rpc_client).await?;

    Ok(())
}

/// Compiles a high-level report from RunTxs.
async fn make_report(
    txs: &[RunTx],
    rpc_client: &EthProvider,
) -> Result<(), Box<dyn std::error::Error>> {
    let trace_data = get_block_trace_data(txs, rpc_client).await?;

    // TODO: add functions for generating each chart, then generate them here
    for t in &trace_data {
        println!("[TRACE] {:?}", t.trace);
        println!("[RECEIPT] {:?}", t.receipt);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct TxTraceReceipt {
    trace: GethTrace,
    receipt: Option<TransactionReceipt>,
}

impl TxTraceReceipt {
    fn new(trace: GethTrace, receipt: Option<TransactionReceipt>) -> Self {
        Self { trace, receipt }
    }
}

async fn get_block_trace_data(
    txs: &[RunTx],
    rpc_client: &EthProvider,
) -> Result<Vec<TxTraceReceipt>, Box<dyn std::error::Error>> {
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
    for block in all_blocks {
        for tx_hash in block.transactions.hashes() {
            println!("tracing tx {:?}", tx_hash);
            // rpc_client.trace_block(block)
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
                .await?;
            println!("here's your trace: {:?}", trace);
            println!("getting receipt for tx {:?}", tx_hash);
            // receipt might fail if we target a non-ETH chain
            // so if it does fail, we just ignore it
            let receipt = rpc_client.get_transaction_receipt(tx_hash).await;
            if let Ok(receipt) = receipt {
                println!("got receipt for tx {:?}", tx_hash);
                all_traces.push(TxTraceReceipt::new(trace, receipt));
            } else {
                println!("ignored non-standard tx {:?}", tx_hash);
            }
        }
    }

    Ok(all_traces)
}

/// Saves RunTxs to `~/.contender/report_{id}.csv`.
fn save_csv_report(id: u64, txs: &[RunTx]) -> Result<(), Box<dyn std::error::Error>> {
    // make path to ~/.contender/report_<id>.csv
    let home_dir = std::env::var("HOME").expect("Could not get home directory");
    let contender_dir = format!("{}/.contender", home_dir);
    std::fs::create_dir_all(&contender_dir)?;
    let out_path = format!("{}/report_{}.csv", contender_dir, id);

    println!("Exporting report for run #{:?} to {:?}", id, out_path);
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}
