#[derive(Debug, Clone)]
pub enum ReportChartId {
    RpcLatency(&'static str),
}

impl std::fmt::Display for ReportChartId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReportChartId::RpcLatency(method) => &format!("{method}_latency"),
        };
        write!(f, "{s}")
    }
}

impl ReportChartId {
    pub fn filename(
        &self,
        start_run_id: u64,
        end_run_id: u64,
        reports_dir: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!(
            "{reports_dir}/{self}_run-{start_run_id}-{end_run_id}.png",
        ))
    }

    pub fn proper_name(&self) -> String {
        match self {
            ReportChartId::RpcLatency(method) => format!("{method} Latency"),
        }
    }
}
