use plotters::{
    chart::ChartBuilder,
    prelude::BitMapBackend,
    series::Histogram,
    style::{full_palette::BLUE, Color},
};

use super::DrawableChart;

pub struct SendTxLatencyChart {
    pub send_tx_latency: Vec<(u32, u32)>,
}
impl SendTxLatencyChart {
    pub fn new(send_tx_latency: Vec<(u32, u32)>) -> Self {
        Self { send_tx_latency }
    }
}

impl DrawableChart for SendTxLatencyChart {
    fn define_chart(
        &self,
        root: &plotters::prelude::DrawingArea<BitMapBackend, plotters::coord::Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let buckets = self.send_tx_latency.iter().map(|b| b.0).collect::<Vec<_>>();
        let max_occurrences = self
            .send_tx_latency
            .iter()
            .map(|b| b.1)
            .max()
            .expect("no time-to-inclusion data found");

        let mut chart = ChartBuilder::on(root)
            .margin(25)
            .x_label_area_size(60)
            .y_label_area_size(60)
            .build_cartesian_2d(0..buckets.len() as u32 - 1, 0..max_occurrences as u32)?;

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15))
            .x_label_offset(0)
            .x_labels(buckets.len())
            .x_desc("Latency (milliseconds)")
            .x_label_formatter(&|v| {
                let bucket = buckets[*v as usize];
                format!("<= {}", bucket)
            })
            .y_desc("# Transactions")
            .draw()?;

        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .data(self.send_tx_latency.to_owned()),
        )?;

        Ok(())
    }
}
