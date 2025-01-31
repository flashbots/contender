use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    drawing::IntoDrawingArea,
    series::Histogram,
    style::{
        full_palette::{BLUE, WHITE},
        Color,
    },
};

use crate::commands::report::TxTraceReceipt;

pub struct TxGasUsedChart {
    gas_used: Vec<u128>,
}

impl Default for TxGasUsedChart {
    fn default() -> Self {
        Self::new()
    }
}

impl TxGasUsedChart {
    fn new() -> Self {
        Self {
            gas_used: Default::default(),
        }
    }

    pub fn build(trace_data: &[TxTraceReceipt]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut chart = TxGasUsedChart::new();

        for t in trace_data {
            let gas = t.receipt.gas_used;
            chart.add_gas_used(gas + (1000 - (gas % 1000)));
        }

        Ok(chart)
    }

    fn add_gas_used(&mut self, gas_used: u128) {
        self.gas_used.push(gas_used);
    }

    pub fn draw(&self, filepath: impl AsRef<str>) -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new(filepath.as_ref(), (1024, 768)).into_drawing_area();
        root.fill(&WHITE)?;

        let max_gas_used = self.gas_used.iter().max().copied().unwrap_or_default();

        let mut gas_used_counts = std::collections::HashMap::new();
        for &gas in &self.gas_used {
            *gas_used_counts.entry(gas).or_insert(0) += 1;
        }
        let highest_peak = gas_used_counts.values().max().unwrap_or(&0);

        let mut chart = ChartBuilder::on(&root)
            .margin(15)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0..max_gas_used + 1,
                0..highest_peak + (5 - (highest_peak % 5)),
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .label_style(("sans-serif", 15))
            .x_desc("Gas Used")
            .x_label_formatter(&|x| {
                if *x >= 1_000_000 {
                    format!("{}M", x / 1_000_000)
                } else if *x >= 1_000 {
                    format!("{}k", x / 1_000)
                } else {
                    format!("{}", x)
                }
            })
            .y_desc("# Transactions")
            .draw()?;

        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .data(self.gas_used.iter().map(|&x| (x, 1))),
        )?;

        println!("saved chart to {}", filepath.as_ref());
        Ok(())
    }
}
