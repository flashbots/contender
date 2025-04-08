use std::collections::BTreeMap;

use super::DrawableChart;
use contender_core::db::RunTx;
use plotters::{
    prelude::Circle,
    series::LineSeries,
    style::{
        full_palette::{GREEN_400, RED_500}, // BLUEGREY_500,
        FontTransform,
        IntoTextStyle,
        ShapeStyle,
    },
};

pub struct PendingTxsChart {
    /// Maps timestamp to number of pending txs
    pending_txs_per_second: BTreeMap<u64, u64>,
}

impl PendingTxsChart {
    pub fn new(run_txs: &[RunTx]) -> Self {
        let mut pending_txs_per_second = BTreeMap::new();
        // get min/max timestamps from run_txs; evaluate max of start_timestamp and end_timestamp
        let (min_timestamp, max_timestamp) =
            run_txs.iter().fold((u64::MAX, 0), |(min, max), tx| {
                let start_timestamp = tx.start_timestamp;
                let end_timestamp = tx.end_timestamp.unwrap_or_default();
                (
                    min.min(start_timestamp).min(end_timestamp),
                    max.max(start_timestamp).max(end_timestamp),
                )
            });

        for t in min_timestamp - 1..max_timestamp + 1 {
            let pending_txs = run_txs
                .iter()
                .filter(|tx| {
                    let start_timestamp = tx.start_timestamp;
                    let end_timestamp = tx.end_timestamp.unwrap_or(u64::MAX);
                    start_timestamp <= t && t < end_timestamp
                })
                .count() as u64;
            pending_txs_per_second.insert(t, pending_txs);
        }

        Self {
            pending_txs_per_second,
        }
    }
}

impl DrawableChart for PendingTxsChart {
    fn define_chart(
        &self,
        root: &plotters::prelude::DrawingArea<
            plotters::prelude::BitMapBackend,
            plotters::coord::Shift,
        >,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let min_timestamp = self
            .pending_txs_per_second
            .keys()
            .min()
            .copied()
            .unwrap_or_default();
        let max_timestamp = self
            .pending_txs_per_second
            .keys()
            .max()
            .copied()
            .unwrap_or_default();
        let max_pending_txs = self
            .pending_txs_per_second
            .values()
            .max()
            .copied()
            .unwrap_or_default();

        let mut chart = plotters::chart::ChartBuilder::on(root)
            .margin(15)
            .x_label_area_size(60)
            .y_label_area_size(60)
            .build_cartesian_2d(
                min_timestamp..max_timestamp + 1,
                0..max_pending_txs + (5 - (max_pending_txs % 5)),
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_desc("Timestamp          ")
            .x_label_formatter(&|timestamp| format!("               {}", timestamp))
            .x_label_style(
                ("sans-serif", 15)
                    .into_text_style(root)
                    .transform(FontTransform::Rotate90),
            )
            .x_labels(25)
            .y_desc("# Pending Transactions")
            .y_labels(25)
            .y_max_light_lines(1)
            .draw()?;

        // draw line chart
        let chart_data = self
            .pending_txs_per_second
            .iter()
            .map(|(timestamp, gas_used)| ({ *timestamp }, *gas_used));
        chart.draw_series(LineSeries::new(chart_data.to_owned(), &GREEN_400))?;

        // draw dots on line chart
        let mk_dot = |c: (u64, u64)| Circle::new(c, 3, Into::<ShapeStyle>::into(RED_500).filled());
        chart.draw_series(chart_data.map(|(x, y)| mk_dot((x, y))))?;

        Ok(())
    }
}
