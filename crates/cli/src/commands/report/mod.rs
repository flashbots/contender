mod block_trace;
mod cache;
mod chart;
mod gen_html;
mod util;

use crate::util::{data_dir, write_run_txs};
use alloy::{providers::ProviderBuilder, transports::http::reqwest::Url};
use block_trace::get_block_trace_data;
use cache::CacheFile;
use chart::ReportChartId;
use chart::{GasPerBlockChart, HeatMapChart, TimeToInclusionChart, TxGasUsedChart};
use contender_core::db::{DbOps, RunTx};
use csv::WriterBuilder;
use gen_html::{build_html_report, ReportMetadata};
use std::str::FromStr;

/// Returns the fully-qualified path to the report directory.
fn report_dir() -> Result<String, Box<dyn std::error::Error>> {
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
    let rpc_client = ProviderBuilder::new().on_http(url);
    let (trace_data, blocks) = get_block_trace_data(&all_txs, &rpc_client).await?;

    // cache data to file
    let cache_data = CacheFile::new(trace_data, blocks);
    cache_data.save()?;

    // make heatmap
    let heatmap = HeatMapChart::build(&cache_data.traces)?;
    heatmap.draw(ReportChartId::Heatmap.filename(start_run_id, end_run_id)?)?;

    // make gasPerBlock chart
    let gas_per_block = GasPerBlockChart::build(&cache_data.blocks);
    gas_per_block.draw(ReportChartId::GasPerBlock.filename(start_run_id, end_run_id)?)?;

    // make timeToInclusion chart
    let time_to_inclusion = TimeToInclusionChart::build(&all_txs);
    time_to_inclusion.draw(ReportChartId::TimeToInclusion.filename(start_run_id, end_run_id)?)?;

    // make txGasUsed chart
    let tx_gas_used = TxGasUsedChart::build(&cache_data.traces)?;
    tx_gas_used.draw(ReportChartId::TxGasUsed.filename(start_run_id, end_run_id)?)?;

    // compile report
    let report_path = build_html_report(ReportMetadata {
        scenario_name: scenario_title,
        start_run_id,
        end_run_id,
        start_block: cache_data.blocks.first().unwrap().header.number,
        end_block: cache_data.blocks.last().unwrap().header.number,
        rpc_url: rpc_url.to_string(),
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
