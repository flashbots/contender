use contender_core::db::RunTx;
use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    series::Histogram,
    style::{full_palette::BLUE, Color},
};

use super::DrawableChart;

pub struct TimeToInclusionChart {
    /// Maps number of times a block was included in a time period.
    inclusion_times: Vec<u64>,
}

impl TimeToInclusionChart {
    pub fn new(run_txs: &[RunTx]) -> Self {
        let mut inclusion_times = vec![];
        for tx in run_txs {
            let mut dumb_base = 0;
            if let Some(end_timestamp) = tx.end_timestamp {
                // dumb_base prevents underflow in case system time doesn't match block timestamps
                if dumb_base == 0 && end_timestamp < tx.start_timestamp {
                    dumb_base += tx.start_timestamp - end_timestamp;
                }
                let end_timestamp = end_timestamp + dumb_base;
                let tti = end_timestamp - tx.start_timestamp;
                inclusion_times.push(tti);
            }
        }
        Self { inclusion_times }
    }
}

impl DrawableChart for TimeToInclusionChart {
    fn define_chart(
        &self,
        root: &plotters::prelude::DrawingArea<BitMapBackend, plotters::coord::Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let min_tti = self
            .inclusion_times
            .iter()
            .min()
            .expect("no time-to-inclusion data found");
        let max_tti = self
            .inclusion_times
            .iter()
            .max()
            .expect("no time-to-inclusion data found");

        let mut chart = ChartBuilder::on(root)
            .margin(15)
            .x_label_area_size(60)
            .y_label_area_size(60)
            .build_cartesian_2d(*min_tti..*max_tti + 1, 0..self.inclusion_times.len() as u32)?;

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15))
            .x_label_offset(10)
            .x_labels(20)
            .x_desc("Time to Inclusion (seconds)")
            .y_desc("# Transactions")
            .draw()?;

        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .data(self.inclusion_times.iter().map(|&x| (x, 1))),
        )?;

        Ok(())
    }
}
