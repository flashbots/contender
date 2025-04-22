use super::block_trace::{get_block_data, get_block_traces};
use super::cache::CacheFile;
use super::chart::{
    GasPerBlockChart, HeatMapChart, LatencyChart, PendingTxsChart, ReportChartId,
    TimeToInclusionChart, TxGasUsedChart,
};
use super::gen_html::{build_html_report, ReportMetadata};
use crate::commands::report::chart::DrawableChart;
use crate::util::{report_dir, write_run_txs};
use alloy::network::AnyNetwork;
use alloy::providers::DynProvider;
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use contender_core::buckets::{Bucket, BucketsExt};
use contender_core::db::{DbOps, RunTx};
use csv::WriterBuilder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

pub async fn report(
    last_run_id: Option<u64>,
    preceding_runs: u64,
    db: &(impl DbOps + Clone + Send + Sync + 'static),
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

    let rpc_url = db
        .get_run(end_run_id)?
        .ok_or_else(|| format!("No run found with ID: {}", end_run_id))?
        .rpc_url;

    // collect CSV report for each run_id
    let start_run_id = end_run_id - preceding_runs;
    let mut all_txs = vec![];
    for id in start_run_id..=end_run_id {
        let txs = db.get_run_txs(id)?;
        all_txs.extend_from_slice(&txs);
        save_csv_report(id, &txs)?;
    }

    // get run data, filter by rpc_url
    let mut run_data = vec![];
    for id in start_run_id..=end_run_id {
        let run = db.get_run(id)?;
        if let Some(run) = run {
            if run.rpc_url != rpc_url {
                continue;
            }
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
    let url = Url::from_str(&rpc_url).expect("Invalid URL");
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

    // cache data to file
    let cache_data = CacheFile::new(trace_data, blocks);
    cache_data.save()?;

    // collect latency data for all relevant methods
    let latency_methods = [
        "eth_sendRawTransaction",
        "eth_blockNumber",
        "eth_chainId",
        "eth_estimateGas",
        "eth_gasPrice",
        "eth_getBlockByNumber",
        "eth_getBlockReceipts",
        "eth_getTransactionCount",
    ];
    let mut canonical_latency_map = BTreeMap::<String, Vec<Bucket>>::new();
    for method in latency_methods {
        // collect latency data from DB for each relevant run
        let mut canonical_latency: Vec<Bucket> = vec![];
        for run_id in start_run_id..=end_run_id {
            let latencies = db.get_latency_metrics(run_id, method)?;
            for bucket in latencies {
                if let Some(entry) = canonical_latency
                    .iter_mut()
                    .find(|b| b.upper_bound == bucket.upper_bound)
                {
                    entry.cumulative_count += bucket.cumulative_count;
                } else {
                    canonical_latency.push(bucket);
                }
            }
        }
        canonical_latency_map.insert(method.to_string(), canonical_latency);
    }

    let chart_ids = vec![
        ReportChartId::Heatmap,
        ReportChartId::GasPerBlock,
        ReportChartId::TimeToInclusion,
        ReportChartId::TxGasUsed,
        ReportChartId::PendingTxs,
        ReportChartId::RpcLatency("eth_sendRawTransaction"),
    ];

    let to_ms = |latency: f64| (latency * 1000.0).round_ties_even() as u64; // convert to ms

    let metrics = SpamRunMetrics {
        peak_gas,
        peak_tx_count,
        average_block_time_secs: average_block_time,
        latency_quantiles: canonical_latency_map
            .iter()
            .map(|(method, latencies)| RpcLatencyQuantiles {
                p50: to_ms(latencies.estimate_quantile(0.5)),
                p90: to_ms(latencies.estimate_quantile(0.9)),
                p99: to_ms(latencies.estimate_quantile(0.99)),
                method: method.to_owned(),
            })
            .collect(),
    };

    // make relevant chart for each report_id
    for chart_id in &chart_ids {
        let filename = chart_id.filename(start_run_id, end_run_id)?;
        let chart: Box<dyn DrawableChart> = match *chart_id {
            ReportChartId::Heatmap => Box::new(HeatMapChart::new(&cache_data.traces)?),
            ReportChartId::GasPerBlock => Box::new(GasPerBlockChart::new(&cache_data.blocks)),
            ReportChartId::TimeToInclusion => Box::new(TimeToInclusionChart::new(&all_txs)),
            ReportChartId::TxGasUsed => Box::new(TxGasUsedChart::new(&cache_data.traces)),
            ReportChartId::PendingTxs => Box::new(PendingTxsChart::new(&all_txs)),
            ReportChartId::RpcLatency(method) => Box::new(LatencyChart::new(
                canonical_latency_map
                    .get(method)
                    .expect("no latency metrics for method")
                    .to_owned(),
            )),
        };
        chart.draw(&filename)?;
    }

    // compile report
    let report_path = build_html_report(ReportMetadata {
        scenario_name: scenario_title,
        start_run_id,
        end_run_id,
        start_block: cache_data.blocks.first().unwrap().header.number,
        end_block: cache_data.blocks.last().unwrap().header.number,
        rpc_url: rpc_url.to_string(),
        metrics,
        chart_ids,
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
/// For display purposes only. Values are in milliseconds.
pub struct RpcLatencyQuantiles {
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
    pub method: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
/// Metrics for a spam run. Must be readable by handlebars.
pub struct SpamRunMetrics {
    pub peak_gas: u64,
    pub peak_tx_count: u64,
    pub average_block_time_secs: f64,
    pub latency_quantiles: Vec<RpcLatencyQuantiles>,
}
