use std::collections::HashMap;

use super::{report_dir, ReportChartId};

pub fn build_html_report(
    start_run_id: u64,
    end_run_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let report_dir = report_dir()?;
    let mut charts = Vec::new();
    for chart_id in &[
        ReportChartId::Heatmap,
        ReportChartId::GasPerBlock,
        ReportChartId::TimeToInclusion,
        ReportChartId::TxGasUsed,
    ] {
        let filename = chart_id.filename(start_run_id, end_run_id)?;
        charts.push((chart_id.proper_name(), filename));
    }

    let mut data = HashMap::new();
    data.insert("charts", charts);

    let template = include_str!("template.html");
    let html = handlebars::Handlebars::new().render_template(template, &data)?;
    let path = format!("{}/report-{}-{}.html", report_dir, start_run_id, end_run_id);
    std::fs::write(&path, html)?;
    println!("saved report to {}", path);

    Ok(())
}
