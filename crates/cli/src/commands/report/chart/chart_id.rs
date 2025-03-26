use crate::util::report_dir;

pub enum ReportChartId {
    Heatmap,
    GasPerBlock,
    TimeToInclusion,
    TxGasUsed,
    PendingTxs,
}

impl std::fmt::Display for ReportChartId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReportChartId::Heatmap => "heatmap",
            ReportChartId::GasPerBlock => "gas_per_block",
            ReportChartId::TimeToInclusion => "time_to_inclusion",
            ReportChartId::TxGasUsed => "tx_gas_used",
            ReportChartId::PendingTxs => "pending_txs",
        };
        write!(f, "{}", s)
    }
}

impl ReportChartId {
    pub fn filename(
        &self,
        start_run_id: u64,
        end_run_id: u64,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok(format!(
            "{}/{}_run-{}-{}.png",
            report_dir()?,
            self,
            start_run_id,
            end_run_id
        ))
    }

    pub fn proper_name(&self) -> String {
        match self {
            ReportChartId::Heatmap => "Storage Slot Heatmap",
            ReportChartId::GasPerBlock => "Gas Per Block",
            ReportChartId::TimeToInclusion => "Time To Inclusion",
            ReportChartId::TxGasUsed => "Tx Gas Used",
            ReportChartId::PendingTxs => "Pending Transactions",
        }
        .to_string()
    }
}
