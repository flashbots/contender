use super::block_trace::{get_block_data, get_block_traces};
use super::cache::CacheFile;
use super::chart::{
    GasPerBlockChart, HeatMapChart, PendingTxsChart, ReportChartId, SendTxLatencyChart,
    TimeToInclusionChart, TxGasUsedChart,
};
use super::gen_html::{build_html_report, ReportMetadata};
use crate::commands::report::chart::DrawableChart;
use crate::util::{report_dir, write_run_txs};
use alloy::network::AnyNetwork;
use alloy::providers::DynProvider;
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use contender_core::db::{DbOps, RunTx};
use csv::WriterBuilder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

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

    // get run data
    let mut run_data = vec![];
    for id in start_run_id..=end_run_id {
        let run = db.get_run(id)?;
        if let Some(run) = run {
            run_data.push(run);
        }
    }
    // collect all unique scenario_name values from run_data
    let scenario_names: Vec<String> = run_data
        .iter()
        .map(|run| run.scenario_name.clone())
        .collect::<std::collections::HashSet<_>>()
        .iter()
        .map(|v| {
            // return only the filename without the path and extension
            let re = regex::Regex::new(r".*/(.*)\.toml$").unwrap();
            re.replace(v, "$1").to_string()
        })
        .collect();
    let scenario_title = scenario_names
        .into_iter()
        .reduce(|acc, v| format!("{}, {}", acc, v))
        .unwrap_or_default();

    // get trace data for reports
    let url = Url::from_str(rpc_url).expect("Invalid URL");
    let rpc_client = DynProvider::new(ProviderBuilder::new().network::<AnyNetwork>().on_http(url));
    let (trace_data, blocks) = if std::env::var("DEBUG_USEFILE").is_ok() {
        println!("DEBUG_USEFILE detected: using cached data");
        // load trace data from file
        let cache_data = CacheFile::load()?;
        (cache_data.traces, cache_data.blocks)
    } else {
        // run traces on RPC
        let block_data = get_block_data(&all_txs, &rpc_client).await?;
        let trace_data = get_block_traces(&block_data, &rpc_client).await?;
        (trace_data, block_data)
    };

    // find peak gas usage
    let peak_gas = blocks.iter().map(|b| b.header.gas_used).max().unwrap_or(0);

    // find peak tx count
    let peak_tx_count = blocks
        .iter()
        .map(|b| b.transactions.len())
        .max()
        .unwrap_or(0) as u64;

    // find average block time
    let mut block_timestamps = blocks
        .iter()
        .map(|b| b.header.timestamp)
        .collect::<Vec<_>>();
    block_timestamps.sort();
    let average_block_time = block_timestamps
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(0, |acc, diff| acc + diff) as f64
        / (blocks.len() - 1).max(1) as f64;

    // load latency data from DB
    let mut canonical_sendtx_latency: BTreeMap<u32, u32> = BTreeMap::new();
    for run_id in start_run_id..=end_run_id {
        let tx_latency = db.get_sendtx_latency(run_id)?;
        for (latency, count) in tx_latency {
            canonical_sendtx_latency
                .entry(latency as u32)
                .and_modify(|e| *e += count as u32)
                .or_insert(count as u32);
        }
    }

    let metrics = SpamRunMetrics {
        peak_gas,
        peak_tx_count,
        average_block_time_secs: average_block_time,
    };

    // cache data to file
    let cache_data = CacheFile::new(trace_data, blocks);
    cache_data.save()?;

    // make heatmap
    let heatmap = HeatMapChart::new(&cache_data.traces)?;
    heatmap.draw(&ReportChartId::Heatmap.filename(start_run_id, end_run_id)?)?;

    // make gasPerBlock chart
    let gas_per_block = GasPerBlockChart::new(&cache_data.blocks);
    gas_per_block.draw(&ReportChartId::GasPerBlock.filename(start_run_id, end_run_id)?)?;

    // make timeToInclusion chart
    let time_to_inclusion = TimeToInclusionChart::new(&all_txs);
    time_to_inclusion.draw(&ReportChartId::TimeToInclusion.filename(start_run_id, end_run_id)?)?;

    // make txGasUsed chart
    let tx_gas_used = TxGasUsedChart::new(&cache_data.traces);
    tx_gas_used.draw(&ReportChartId::TxGasUsed.filename(start_run_id, end_run_id)?)?;

    // make pendingTxs chart
    let pending_txs = PendingTxsChart::new(&all_txs);
    pending_txs.draw(&ReportChartId::PendingTxs.filename(start_run_id, end_run_id)?)?;

    // make sendTxLatency chart
    let send_tx_latency = SendTxLatencyChart::new(
        canonical_sendtx_latency
            .iter()
            .map(|(x, y)| (*x, *y))
            .collect::<Vec<_>>(),
    );
    send_tx_latency.draw(&ReportChartId::SendTxLatency.filename(start_run_id, end_run_id)?)?;

    // compile report
    let report_path = build_html_report(ReportMetadata {
        scenario_name: scenario_title,
        start_run_id,
        end_run_id,
        start_block: cache_data.blocks.first().unwrap().header.number,
        end_block: cache_data.blocks.last().unwrap().header.number,
        rpc_url: rpc_url.to_string(),
        metrics,
    })?;

    // Open the report in the default web browser
    webbrowser::open(&report_path)?;

    Ok(())
}

/// Saves RunTxs to `{data_dir}/reports/{id}.csv`.
fn save_csv_report(id: u64, txs: &[RunTx]) -> Result<(), Box<dyn std::error::Error>> {
    let report_dir = report_dir()?;
    let out_path = format!("{report_dir}/{id}.csv");

    println!("Exporting report for run #{:?} to {:?}", id, out_path);
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpamRunMetrics {
    pub peak_gas: u64,
    pub peak_tx_count: u64,
    pub average_block_time_secs: f64,
}
