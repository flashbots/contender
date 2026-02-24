use crate::chart::flashblock_time_to_inclusion::FlashblockTimeToInclusionData;
use crate::chart::pending_txs::PendingTxsData;
use crate::chart::rpc_latency::LatencyData;
use crate::chart::time_to_inclusion::TimeToInclusionData;
use crate::chart::tx_gas_used::TxGasUsedData;
use crate::chart::{gas_per_block::GasPerBlockData, heatmap::HeatmapData};
use crate::command::SpamRunMetrics;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct ReportMetadata {
    pub scenario_name: String,
    pub start_run_id: u64,
    pub end_run_id: u64,
    pub start_block: u64,
    pub end_block: u64,
    pub rpc_url: String,
    pub metrics: SpamRunMetrics,
    pub chart_data: ChartData,
    pub campaign: Option<CampaignMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CampaignMetadata {
    pub id: Option<String>,
    pub name: Option<String>,
    pub stage: Option<String>,
    pub scenario: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChartData {
    pub gas_per_block: GasPerBlockData,
    pub heatmap: HeatmapData,
    pub time_to_inclusion: TimeToInclusionData,
    pub tx_gas_used: TxGasUsedData,
    pub pending_txs: PendingTxsData,
    pub latency_data_sendrawtransaction: LatencyData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flashblock_time_to_inclusion: Option<FlashblockTimeToInclusionData>,
}

#[derive(Deserialize, Serialize)]
struct TemplateData {
    scenario_name: String,
    date: String,
    rpc_url: String,
    start_block: String,
    end_block: String,
    metrics: SpamRunMetrics,
    chart_data: ChartData,
    campaign: Option<CampaignMetadata>,
    version: String,
}

impl TemplateData {
    pub fn new(meta: &ReportMetadata) -> Self {
        Self {
            scenario_name: meta.scenario_name.to_owned(),
            date: chrono::Local::now().to_rfc2822(),
            rpc_url: meta.rpc_url.to_owned(),
            start_block: meta.start_block.to_string(),
            end_block: meta.end_block.to_string(),
            metrics: meta.metrics.to_owned(),
            chart_data: meta.chart_data.to_owned(),
            campaign: meta.campaign.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Stable JSON export format for single-run reports.
/// This struct is versioned to allow backward-compatible changes.
#[derive(Serialize)]
pub struct ReportExportV1 {
    /// Export format version (always 1 for this struct)
    pub export_version: u32,
    /// Contender version that generated this report
    pub version: String,
    /// RFC3339 timestamp when the report was generated
    pub generated_at: String,
    /// Scenario name(s) included in this report
    pub scenario_name: String,
    /// RPC URL used for the spam run
    pub rpc_url: String,
    /// First run ID included in this report
    pub start_run_id: u64,
    /// Last run ID included in this report
    pub end_run_id: u64,
    /// First block number in the report range
    pub start_block: u64,
    /// Last block number in the report range
    pub end_block: u64,
    /// Spam run metrics (peak gas, latency quantiles, etc.)
    pub metrics: SpamRunMetrics,
    /// Chart data for visualization
    pub chart_data: ChartData,
    /// Campaign metadata if this run is part of a campaign
    #[serde(skip_serializing_if = "Option::is_none")]
    pub campaign: Option<CampaignMetadata>,
}

impl ReportExportV1 {
    pub fn new(meta: &ReportMetadata) -> Self {
        Self {
            export_version: 1,
            version: env!("CARGO_PKG_VERSION").to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            scenario_name: meta.scenario_name.clone(),
            rpc_url: meta.rpc_url.clone(),
            start_run_id: meta.start_run_id,
            end_run_id: meta.end_run_id,
            start_block: meta.start_block,
            end_block: meta.end_block,
            metrics: meta.metrics.clone(),
            chart_data: meta.chart_data.clone(),
            campaign: meta.campaign.clone(),
        }
    }
}

/// Builds an HTML report for the given run IDs. Returns the path to the report.
pub fn build_html_report(meta: ReportMetadata, reports_dir: &Path) -> Result<PathBuf> {
    let template = include_str!("template.html.handlebars");

    let mut data = HashMap::new();
    let template_data = TemplateData::new(&meta);
    data.insert("data", template_data);
    let html = handlebars::Handlebars::new().render_template(template, &data)?;

    let filename = format!("report-{}-{}.html", meta.start_run_id, meta.end_run_id);
    let path = reports_dir.join(filename);
    std::fs::write(&path, html)?;

    Ok(path)
}

/// Builds a JSON report for the given run IDs. Returns the path to the report.
pub fn build_json_report(meta: &ReportMetadata, reports_dir: &Path) -> Result<PathBuf> {
    let export = ReportExportV1::new(meta);
    let json = serde_json::to_string_pretty(&export)?;

    let filename = format!("report-{}-{}.json", meta.start_run_id, meta.end_run_id);
    let path = reports_dir.join(filename);
    std::fs::write(&path, json)?;

    Ok(path)
}
