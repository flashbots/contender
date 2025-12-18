use crate::chart::pending_txs::PendingTxsData;
use crate::chart::rpc_latency::LatencyData;
use crate::chart::time_to_inclusion::TimeToInclusionData;
use crate::chart::tx_gas_used::TxGasUsedData;
use crate::chart::{gas_per_block::GasPerBlockData, heatmap::HeatmapData};
use crate::command::SpamRunMetrics;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

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
        }
    }
}

/// Builds an HTML report for the given run IDs. Returns the path to the report.
pub fn build_html_report(meta: ReportMetadata, reports_dir: &str) -> Result<String> {
    let template = include_str!("template.html.handlebars");

    let mut data = HashMap::new();
    let template_data = TemplateData::new(&meta);
    data.insert("data", template_data);
    let html = handlebars::Handlebars::new().render_template(template, &data)?;

    let path = format!(
        "{}/report-{}-{}.html",
        reports_dir, meta.start_run_id, meta.end_run_id
    );
    std::fs::write(&path, html)?;
    info!("saved report to {path}");

    Ok(path)
}
