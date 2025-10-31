use super::gen_html::{build_html_report, ReportMetadata};
use super::util::std_deviation;
use crate::block_trace::{estimate_block_data, get_block_data, get_block_traces};
use crate::cache::CacheFile;
use crate::chart::{
    gas_per_block::GasPerBlockChart, heatmap::HeatMapChart, pending_txs::PendingTxsChart,
    rpc_latency::LatencyChart, time_to_inclusion::TimeToInclusionChart,
    tx_gas_used::TxGasUsedChart,
};
use crate::gen_html::ChartData;
use crate::util::write_run_txs;
use alloy::network::AnyNetwork;
use alloy::providers::DynProvider;
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use contender_core::buckets::{Bucket, BucketsExt};
use contender_core::db::{DbOps, RunTx};
use contender_core::error::ContenderError;
use csv::WriterBuilder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use tracing::{debug, info};

pub async fn report(
    last_run_id: Option<u64>,
    preceding_runs: u64,
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    data_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_runs = db.num_runs()?;

    let data_path = Path::new(data_dir).join("reports");
    if !data_path.exists() {
        fs::create_dir_all(data_path)?;
    }

    if num_runs == 0 {
        info!("No runs found in the database. Exiting.");
        return Ok(());
    }

    // if id is provided, check if it's valid
    let end_run_id = if let Some(id) = last_run_id {
        if id == 0 || id > num_runs {
            // panic!("Invalid run ID: {}", id);
            return Err(format!("Invalid run ID: {id}").into());
        }
        id
    } else {
        // get latest run
        info!("No run ID provided. Using latest run ID: {num_runs}");
        num_runs
    };

    let rpc_url = db
        .get_run(end_run_id)?
        .ok_or_else(|| format!("No run found with ID: {end_run_id}"))?
        .rpc_url;

    // collect CSV report for each run_id
    let start_run_id = end_run_id - preceding_runs;
    let mut all_txs = vec![];
    for id in start_run_id..=end_run_id {
        let txs = db.get_run_txs(id)?;
        all_txs.extend_from_slice(&txs);
        save_csv_report(id, &txs, &format!("{data_dir}/reports"))?;
    }

    // get run data, filter by rpc_url
    let mut run_data = vec![];
    let mut runtime_params_list = Vec::new();
    for id in start_run_id..=end_run_id {
        let run = db.get_run(id)?;
        if let Some(run) = run {
            if run.rpc_url != rpc_url {
                continue;
            }
            runtime_params_list.push(RuntimeParams {
                txs_per_duration: run.txs_per_duration,
                duration_value: run.duration.value(),
                duration_unit: run.duration.unit().to_owned(),
                timeout: run.timeout,
            });
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
        .reduce(|acc, v| format!("{acc}, {v}"))
        .unwrap_or_default();

    // get trace data for reports
    let url = Url::from_str(&rpc_url).expect("Invalid URL");
    let rpc_client = DynProvider::new(
        ProviderBuilder::new()
            .network::<AnyNetwork>()
            .connect_http(url),
    );
    let (trace_data, blocks) = if std::env::var("DEBUG_USEFILE").is_ok() {
        info!("DEBUG_USEFILE detected: using cached data");
        // load trace data from file
        let cache_data = CacheFile::load(data_dir)?;
        (cache_data.traces, cache_data.blocks)
    } else {
        // run traces on RPC
        let block_data = if all_txs.is_empty() {
            debug!("No transactions found, estimating blocks from DB runs");
            estimate_block_data(start_run_id, end_run_id, &rpc_client, db).await?
        } else {
            get_block_data(&all_txs, &rpc_client).await?
        };
        let trace_data = get_block_traces(&block_data, &rpc_client).await?;
        (trace_data, block_data)
    };

    // find peak gas usage
    let peak_gas = blocks.iter().map(|b| b.header.gas_used).max().unwrap_or(0);
    let block_gas_limit = blocks
        .first()
        .map(|blk| blk.header.gas_limit)
        .unwrap_or(30_000_000);

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
        .sum::<u64>() as f64
        / (blocks.len() - 1).max(1) as f64;
    let block_time_delta_std_dev = std_deviation(
        &block_timestamps
            .windows(2)
            .map(|w| w[1] - w[0])
            .collect::<Vec<_>>(),
    );

    // cache data to file
    let cache_data = CacheFile::new(trace_data, blocks, data_dir);
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
        "eth_sendBundle",
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

    let to_ms = |latency: f64| (latency * 1000.0).round_ties_even() as u64; // convert to ms

    let block_time_delta_std_dev = block_time_delta_std_dev.unwrap_or(0.0);
    let metrics = SpamRunMetrics {
        peak_gas: MetricDescriptor::new(
            peak_gas,
            Some(&format!("{}%", (peak_gas * 100) / block_gas_limit)),
        ),
        peak_tx_count: MetricDescriptor::new(peak_tx_count, None),
        average_block_time_secs: MetricDescriptor::new(
            average_block_time,
            Some(&if block_time_delta_std_dev == 0.0 {
                "stable".to_owned()
            } else {
                format!("unstable (\u{03c3}={block_time_delta_std_dev:.2})")
            }),
        ),
        latency_quantiles: canonical_latency_map
            .iter()
            .map(|(method, latencies)| RpcLatencyQuantiles {
                p50: to_ms(latencies.estimate_quantile(0.5)),
                p90: to_ms(latencies.estimate_quantile(0.9)),
                p99: to_ms(latencies.estimate_quantile(0.99)),
                method: method.to_owned(),
            })
            .collect(),
        runtime_params: runtime_params_list,
    };

    let heatmap = HeatMapChart::new(&cache_data.traces)?;
    let gas_per_block = GasPerBlockChart::new(&cache_data.blocks);
    let tti = TimeToInclusionChart::new(&all_txs);
    let gas_used = TxGasUsedChart::new(&cache_data.traces, 4000);
    let pending_txs = PendingTxsChart::new(&all_txs);
    let latency_chart_sendrawtx = LatencyChart::new(
        canonical_latency_map
            .get("eth_sendRawTransaction")
            .ok_or(ContenderError::GenericError(
                "no latency metrics for eth_sendRawTransaction",
                "".to_owned(),
            ))?
            .to_owned(),
    );

    // compile report
    let mut blocks = cache_data.blocks;
    blocks.sort_by(|a, b| a.header.number.cmp(&b.header.number));
    let report_path = build_html_report(
        ReportMetadata {
            scenario_name: scenario_title,
            start_run_id,
            end_run_id,
            start_block: blocks.first().unwrap().header.number,
            end_block: blocks.last().unwrap().header.number,
            rpc_url: rpc_url.to_string(),
            metrics,
            chart_data: ChartData {
                heatmap: heatmap.echart_data(),
                gas_per_block: gas_per_block.echart_data(),
                time_to_inclusion: tti.echart_data(),
                tx_gas_used: gas_used.echart_data(),
                pending_txs: pending_txs.echart_data(),
                latency_data_sendrawtransaction: latency_chart_sendrawtx.echart_data(),
            },
        },
        &format!("{data_dir}/reports"),
    )?;

    // Open the report in the default web browser, skipping if "none" is set
    // in the BROWSER environment variable.
    // This is useful for CI environments where we don't want to open a browser.
    if env::var("BROWSER").unwrap_or_default() == "none" {
        return Ok(());
    }
    webbrowser::open(&report_path)?;

    Ok(())
}

/// Saves RunTxs to `{reports_dir}/{id}.csv`.
fn save_csv_report(
    id: u64,
    txs: &[RunTx],
    reports_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_path = format!("{reports_dir}/{id}.csv");

    info!("Exporting report for run #{id:?} to {out_path:?}");
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
    pub peak_gas: MetricDescriptor<u64>,
    pub peak_tx_count: MetricDescriptor<u64>,
    pub average_block_time_secs: MetricDescriptor<f64>,
    pub latency_quantiles: Vec<RpcLatencyQuantiles>,
    pub runtime_params: Vec<RuntimeParams>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RuntimeParams {
    pub txs_per_duration: u64,
    pub duration_value: u64,
    pub duration_unit: String,
    pub timeout: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MetricDescriptor<T> {
    pub description: Option<String>,
    pub value: T,
}

impl<T> MetricDescriptor<T> {
    pub fn new(value: T, description: Option<&str>) -> Self {
        Self {
            description: description.map(String::from),
            value,
        }
    }
}
