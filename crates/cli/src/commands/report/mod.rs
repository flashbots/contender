mod heatmap;

use crate::util::{data_dir, write_run_txs};
use alloy::providers::ext::DebugApi;
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
use contender_core::{
    db::{DbOps, RunTx},
    generator::types::EthProvider,
};
use csv::WriterBuilder;
use heatmap::HeatMapBuilder;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

static CACHE_FILENAME: &str = "debug_trace.json";

fn cache_path() -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!("{}/{}", data_dir()?, CACHE_FILENAME))
}

fn report_path() -> Result<String, Box<dyn std::error::Error>> {
    let path = format!("{}/reports", data_dir()?);
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

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

    // get trace data for reports
    let trace_data = get_block_trace_data(&all_txs, &rpc_client).await?;

    // make heatmap
    let heatmap = HeatMapBuilder::new().build(&trace_data)?;
    heatmap.draw(format!(
        "{}/heatmap-run-{}-{}.png",
        report_path()?,
        start_run_id,
        end_run_id
    ))?;

    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TxTraceReceipt {
    trace: GethTrace,
    receipt: TransactionReceipt,
}

impl TxTraceReceipt {
    fn new(trace: GethTrace, receipt: TransactionReceipt) -> Self {
        Self { trace, receipt }
    }
}

async fn get_block_trace_data(
    txs: &[RunTx],
    rpc_client: &EthProvider,
) -> Result<Vec<TxTraceReceipt>, Box<dyn std::error::Error>> {
    if std::env::var("DEBUG_USEFILE").is_ok() {
        // load trace data from file
        let path = cache_path()?;
        println!("DEBUG: using data stored at {}", path);
        let file = std::fs::File::open(path)?;
        let traces: Vec<TxTraceReceipt> = serde_json::from_reader(file)?;
        return Ok(traces);
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

    // write all_traces to file
    let file = std::fs::File::create(cache_path()?)?;
    serde_json::to_writer(file, &all_traces)?;

    Ok(all_traces)
}

/// Saves RunTxs to `{data_dir}/reports/{id}.csv`.
fn save_csv_report(id: u64, txs: &[RunTx]) -> Result<(), Box<dyn std::error::Error>> {
    let report_dir = report_path()?;
    let out_path = format!("{report_dir}/{id}.csv");

    println!("Exporting report for run #{:?} to {:?}", id, out_path);
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}
