use super::gen_html::{build_html_report, CampaignMetadata, ReportMetadata};
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
use crate::{Error, Result};
use alloy::network::AnyNetwork;
use alloy::providers::DynProvider;
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use contender_core::buckets::{Bucket, BucketsExt};
use contender_core::db::SpamRun;
use contender_core::db::{DbOps, RunTx};
use csv::WriterBuilder;
use serde::{Deserialize, Serialize};
use serde_json;
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
) -> Result<()> {
    let num_runs = db.num_runs().map_err(|e| e.into())?;

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
            return Err(Error::InvalidRunId(id));
        }
        id
    } else {
        // get latest run
        info!("No run ID provided. Using latest run ID: {num_runs}");
        num_runs
    };

    let rpc_url = db
        .get_run(end_run_id)
        .map_err(|e| e.into())?
        .ok_or(Error::RunDoesNotExist(end_run_id))?
        .rpc_url;

    // collect CSV report for each run_id
    let start_run_id = end_run_id - preceding_runs;
    let mut all_txs = vec![];
    for id in start_run_id..=end_run_id {
        let txs = db.get_run_txs(id).map_err(|e| e.into())?;
        all_txs.extend_from_slice(&txs);
        save_csv_report(id, &txs, &format!("{data_dir}/reports"))?;
    }

    // get run data, filter by rpc_url
    let mut run_data = vec![];
    let mut runtime_params_list = Vec::new();
    for id in start_run_id..=end_run_id {
        let run = db.get_run(id).map_err(|e| e.into())?;
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
    let campaign_context = run_data.iter().rev().find_map(|run| {
        run.campaign_id
            .as_ref()
            .map(|campaign_id| CampaignMetadata {
                id: Some(campaign_id.to_owned()),
                name: run.campaign_name.clone(),
                stage: run.stage_name.clone(),
                scenario: Some(run.scenario_name.clone()),
            })
    });
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
            let latencies = db
                .get_latency_metrics(run_id, method)
                .map_err(|e| e.into())?;
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
            .ok_or(Error::LatencyMetricsEmpty(
                "eth_sendRawTransaction".to_owned(),
            ))?
            .to_owned(),
    );

    // compile report
    let mut blocks = cache_data.blocks;
    blocks.sort_by_key(|a| a.header.number);
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
            campaign: campaign_context,
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
fn save_csv_report(id: u64, txs: &[RunTx], reports_dir: &str) -> Result<()> {
    let out_path = format!("{reports_dir}/{id}.csv");

    info!("Exporting report for run #{id:?} to {out_path:?}");
    let mut writer = WriterBuilder::new().has_headers(true).from_path(out_path)?;
    write_run_txs(&mut writer, txs)?;

    Ok(())
}

#[derive(Clone, Debug, Serialize)]
struct CampaignRunSummary {
    run_id: u64,
    scenario_name: String,
    stage_name: Option<String>,
    tx_count: usize,
    duration: String,
    report_path: String,
}

#[derive(Clone, Debug, Serialize)]
struct CampaignReportSummary {
    campaign_id: String,
    campaign_name: Option<String>,
    runs: Vec<CampaignRunSummary>,
    totals_by_stage: BTreeMap<String, BTreeMap<String, usize>>,
    overall: Option<CampaignOverall>,
    stage_scenario: Vec<StageScenarioSummary>,
    logs_incomplete: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CampaignOverall {
    total_tx_count: u64,
    total_error_count: u64,
    error_rate: f64,
    campaign_start_time: Option<String>,
    campaign_end_time: Option<String>,
    campaign_duration_secs: u64,
    avg_tps: f64,
}

#[derive(Clone, Debug, Serialize)]
struct StageScenarioSummary {
    stage_name: String,
    scenario_name: String,
    total_tx_count: u64,
    total_error_count: u64,
    error_rate: f64,
    duration_secs: u64,
    avg_tps: f64,
}

pub async fn report_campaign(
    campaign_id: &str,
    db: &(impl DbOps + Clone + Send + Sync + 'static),
    data_dir: &str,
) -> Result<()> {
    let runs = db.get_runs_by_campaign(campaign_id).map_err(|e| e.into())?;
    if runs.is_empty() {
        return Err(Error::CampaignNotFound(campaign_id.to_owned()));
    }

    let data_path = Path::new(data_dir).join("reports");
    if !data_path.exists() {
        fs::create_dir_all(&data_path)?;
    }

    let mut summaries = Vec::new();
    let mut totals: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let campaign_name = runs.first().and_then(|r| r.campaign_name.clone());
    let mut overall_acc = OverallAccumulator::default();
    let mut stage_acc: BTreeMap<(String, String), StageAccumulator> = BTreeMap::new();
    let mut logs_incomplete = false;

    let previous_browser = env::var("BROWSER").ok();
    // Avoid opening a browser for every per-run report when generating a campaign summary.
    env::set_var("BROWSER", "none");

    let run_generation_result: Result<()> = async {
        for run in &runs {
            // generate per-run report (single run)
            report(Some(run.id), 0, db, data_dir).await?;
            let run_txs = db.get_run_txs(run.id).map_err(|e| e.into())?;
            let (run_tx_count_from_logs, run_error_count_from_logs) =
                tx_and_error_counts(&run_txs, run.tx_count);
            let logs_complete =
                !run_txs.is_empty() && (run_tx_count_from_logs as usize) >= run.tx_count;
            if !logs_complete {
                logs_incomplete = true;
            }

            let run_tx_count: u64 = if logs_complete {
                run_tx_count_from_logs
            } else {
                run.tx_count as u64
            };
            let run_error_count: u64 = if logs_complete {
                run_error_count_from_logs
            } else {
                0
            };

            let (start_ms, end_ms) = if logs_complete {
                run_time_bounds(run, &run_txs)
            } else {
                run_time_bounds(run, &[])
            };

            overall_acc.add_run(run_tx_count, run_error_count, start_ms, end_ms);

            let stage_key = run
                .stage_name
                .clone()
                .unwrap_or_else(|| "unspecified-stage".to_string());
            let scenario_key = run.scenario_name.clone();

            stage_acc
                .entry((stage_key.clone(), scenario_key.clone()))
                .or_default()
                .add_run(run_tx_count, run_error_count, start_ms, end_ms);

            let stage_key = run
                .stage_name
                .clone()
                .unwrap_or_else(|| "unspecified-stage".to_string());
            let scenario_key = run.scenario_name.clone();
            totals
                .entry(stage_key.clone())
                .or_default()
                .entry(scenario_key.clone())
                .and_modify(|count| *count += run_tx_count as usize)
                .or_insert(run_tx_count as usize);

            let report_file = format!("report-{}-{}.html", run.id, run.id);
            summaries.push(CampaignRunSummary {
                run_id: run.id,
                scenario_name: run.scenario_name.clone(),
                stage_name: run.stage_name.clone(),
                tx_count: run_tx_count as usize,
                duration: run.duration.to_string(),
                report_path: report_file,
            });
        }
        Ok(())
    }
    .await;
    if let Some(prev) = previous_browser {
        env::set_var("BROWSER", prev);
    } else {
        env::remove_var("BROWSER");
    }
    run_generation_result?;

    let summary = CampaignReportSummary {
        campaign_id: campaign_id.to_owned(),
        campaign_name,
        runs: summaries,
        totals_by_stage: totals,
        overall: Some(overall_acc.into_overall()),
        stage_scenario: stage_acc
            .into_iter()
            .map(|((stage, scenario), acc)| acc.into_summary(stage, scenario))
            .collect(),
        logs_incomplete,
    };

    let index_path = data_path.join(format!("campaign-{campaign_id}.html"));
    let html = render_campaign_html(&summary)?;
    fs::write(&index_path, html)?;

    let summary_path = data_path.join(format!("campaign-{campaign_id}.json"));
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;

    info!(
        campaign_id = %campaign_id,
        html = %index_path.display(),
        json = %summary_path.display(),
        "Generated campaign report"
    );

    Ok(())
}

fn render_campaign_html(summary: &CampaignReportSummary) -> Result<String> {
    let template = include_str!("template_campaign.html.handlebars");
    let html = handlebars::Handlebars::new().render_template(
        template,
        &serde_json::json!({
            "campaign": summary,
            "version": env!("CARGO_PKG_VERSION")
        }),
    )?;
    Ok(html)
}

#[derive(Default)]
struct OverallAccumulator {
    total_tx: u64,
    total_errors: u64,
    start_ms: Option<u128>,
    end_ms: Option<u128>,
}

impl OverallAccumulator {
    fn add_run(&mut self, tx: u64, errors: u64, start_ms: Option<u128>, end_ms: Option<u128>) {
        self.total_tx = self.total_tx.saturating_add(tx);
        self.total_errors = self.total_errors.saturating_add(errors);
        if let Some(s) = start_ms {
            self.start_ms = Some(self.start_ms.map_or(s, |curr| curr.min(s)));
        }
        if let Some(e) = end_ms {
            self.end_ms = Some(self.end_ms.map_or(e, |curr| curr.max(e)));
        }
    }

    fn into_overall(self) -> CampaignOverall {
        let duration_secs = match (self.start_ms, self.end_ms) {
            (Some(s), Some(e)) if e > s => ((e - s) / 1000) as u64,
            _ => 0,
        };
        let avg_tps = if duration_secs > 0 {
            let raw = self.total_tx as f64 / duration_secs as f64;
            (raw * 100.0).round() / 100.0 // Round to 2 decimal places
        } else {
            0.0
        };
        let error_rate = if self.total_tx > 0 {
            let raw = self.total_errors as f64 / self.total_tx as f64;
            (raw * 100.0).round() / 100.0 // Round to 2 decimal places
        } else {
            0.0
        };
        CampaignOverall {
            total_tx_count: self.total_tx,
            total_error_count: self.total_errors,
            error_rate,
            campaign_start_time: self.start_ms.map(|v| millis_to_rfc3339(v as i64)),
            campaign_end_time: self.end_ms.map(|v| millis_to_rfc3339(v as i64)),
            campaign_duration_secs: duration_secs,
            avg_tps,
        }
    }
}

#[derive(Default)]
struct StageAccumulator {
    total_tx: u64,
    total_errors: u64,
    start_ms: Option<u128>,
    end_ms: Option<u128>,
}

impl StageAccumulator {
    fn add_run(&mut self, tx: u64, errors: u64, start_ms: Option<u128>, end_ms: Option<u128>) {
        self.total_tx = self.total_tx.saturating_add(tx);
        self.total_errors = self.total_errors.saturating_add(errors);
        if let Some(s) = start_ms {
            self.start_ms = Some(self.start_ms.map_or(s, |curr| curr.min(s)));
        }
        if let Some(e) = end_ms {
            self.end_ms = Some(self.end_ms.map_or(e, |curr| curr.max(e)));
        }
    }

    fn into_summary(self, stage: String, scenario: String) -> StageScenarioSummary {
        let duration_secs = match (self.start_ms, self.end_ms) {
            (Some(s), Some(e)) if e > s => ((e - s) / 1000) as u64,
            _ => 0,
        };
        let avg_tps = if duration_secs > 0 {
            let raw = self.total_tx as f64 / duration_secs as f64;
            (raw * 100.0).round() / 100.0 // Round to 2 decimal places
        } else {
            0.0
        };
        let error_rate = if self.total_tx > 0 {
            let raw = self.total_errors as f64 / self.total_tx as f64;
            (raw * 100.0).round() / 100.0 // Round to 2 decimal places
        } else {
            0.0
        };
        StageScenarioSummary {
            stage_name: stage,
            scenario_name: scenario,
            total_tx_count: self.total_tx,
            total_error_count: self.total_errors,
            error_rate,
            duration_secs,
            avg_tps,
        }
    }
}

fn millis_to_rfc3339(ms: i64) -> String {
    use chrono::Utc;
    let dt = chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(|| chrono::DateTime::<Utc>::from_timestamp_millis(0).unwrap());
    dt.to_rfc3339()
}

fn tx_and_error_counts(run_txs: &[RunTx], fallback_tx_count: usize) -> (u64, u64) {
    if run_txs.is_empty() {
        return (fallback_tx_count as u64, 0);
    }
    let tx_count = run_txs.len() as u64;
    let error_count = run_txs.iter().filter(|tx| tx.error.is_some()).count() as u64;
    (tx_count, error_count)
}

fn run_time_bounds(run: &SpamRun, run_txs: &[RunTx]) -> (Option<u128>, Option<u128>) {
    if !run_txs.is_empty() {
        let start = run_txs
            .iter()
            .map(|t| t.start_timestamp_secs as u128 * 1000)
            .min();
        let end = run_txs
            .iter()
            .map(|t| {
                t.end_timestamp_secs
                    .map(|e| e as u128 * 1000)
                    .unwrap_or(t.start_timestamp_secs as u128 * 1000)
            })
            .max();
        return (start, end);
    }
    let start_ms = run.timestamp as u128;
    let duration_ms = if run.duration.is_seconds() {
        run.duration.value().saturating_mul(1000) as u128
    } else {
        // fallback: treat blocks as seconds for rough duration if not seconds
        run.duration.value().saturating_mul(1000) as u128
    };
    let end_ms = start_ms.saturating_add(duration_ms);
    (Some(start_ms), Some(end_ms))
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
