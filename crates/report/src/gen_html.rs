use crate::chart::ReportChartId;
use crate::chart::{gas_per_block::GasPerBlockData, heatmap::HeatmapData};
use crate::command::SpamRunMetrics;
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
    pub chart_ids: Vec<ReportChartId>,
    pub chart_data: ChartData,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChartData {
    pub gas_per_block: GasPerBlockData,
    pub heatmap: HeatmapData,
}

#[derive(Deserialize, Serialize)]
struct TemplateData {
    scenario_name: String,
    date: String,
    rpc_url: String,
    start_block: String,
    end_block: String,
    metrics: SpamRunMetrics,
    charts: Vec<(String, String)>,
    chart_data: ChartData,
}

impl TemplateData {
    pub fn new(meta: &ReportMetadata, charts: Vec<(String, String)>) -> Self {
        Self {
            scenario_name: meta.scenario_name.to_owned(),
            date: chrono::Local::now().to_rfc2822(),
            rpc_url: meta.rpc_url.to_owned(),
            start_block: meta.start_block.to_string(),
            end_block: meta.end_block.to_string(),
            metrics: meta.metrics.to_owned(),
            charts,
            chart_data: meta.chart_data.to_owned(),
        }
    }
}

/// Builds an HTML report for the given run IDs. Returns the path to the report.
pub fn build_html_report(
    meta: ReportMetadata,
    reports_dir: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut charts = Vec::new();
    for chart_id in &meta.chart_ids {
        let filename = chart_id.filename(meta.start_run_id, meta.end_run_id, reports_dir)?;
        charts.push((chart_id.proper_name(), filename));
    }

    let template = include_str!("template.html.handlebars");

    let mut data = HashMap::new();
    let template_data = TemplateData::new(&meta, charts);
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
