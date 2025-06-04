use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    series::Histogram,
    style::{full_palette::BLUE, Color},
};

use crate::{block_trace::TxTraceReceipt, util::abbreviate_num};

use super::DrawableChart;

pub struct TxGasUsedChart {
    gas_used: Vec<u64>,
}

impl TxGasUsedChart {
    pub fn new(trace_data: &[TxTraceReceipt]) -> Self {
        let mut gas_used = vec![];
        for t in trace_data {
            let gas = t.receipt.gas_used;
            gas_used.push(gas + (1000 - (gas % 1000)));
        }
        Self { gas_used }
    }
}

impl DrawableChart for TxGasUsedChart {
    fn define_chart(
        &self,
        root: &plotters::prelude::DrawingArea<BitMapBackend, plotters::coord::Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_gas_used = self.gas_used.iter().max().copied().unwrap_or_default();

        let mut gas_used_counts = std::collections::HashMap::new();
        for &gas in &self.gas_used {
            *gas_used_counts.entry(gas).or_insert(0) += 1;
        }
        let highest_peak = gas_used_counts.values().max().unwrap_or(&0);

        let mut chart = ChartBuilder::on(root)
            .margin(15)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0..max_gas_used + (10_000 - (max_gas_used % 10_000)),
                0..highest_peak + (5 - (highest_peak % 5)),
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .label_style(("sans-serif", 15))
            .x_desc("Gas Used")
            .x_labels(20)
            .x_label_formatter(&|x| abbreviate_num(*x))
            .y_desc("# Transactions")
            .draw()?;

        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .data(self.gas_used.iter().map(|&x| (x, 1))),
        )?;

        Ok(())
    }
}
