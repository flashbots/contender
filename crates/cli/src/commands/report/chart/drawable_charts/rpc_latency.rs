use super::DrawableChart;
use contender_core::buckets::{Bucket, BucketsExt};
use plotters::{
    chart::ChartBuilder,
    prelude::{BitMapBackend, PathElement, Rectangle},
    style::{
        full_palette::{BLACK, BLUE, RED, WHITE},
        Color,
    },
};

pub struct LatencyChart {
    buckets: Vec<Bucket>,
}

impl LatencyChart {
    pub fn new(buckets: Vec<Bucket>) -> Self {
        Self { buckets }
    }
}

impl DrawableChart for LatencyChart {
    fn define_chart(
        &self,
        root: &plotters::prelude::DrawingArea<BitMapBackend, plotters::coord::Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let max_x = self.buckets.last().unwrap().upper_bound;
        let max_y = self.buckets.last().unwrap().cumulative_count + 1;

        let mut chart = ChartBuilder::on(root)
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(40)
            .build_cartesian_2d(0.0..max_x, 0..max_y)?;

        chart
            .configure_mesh()
            .x_desc("Observed Latency (seconds)")
            .y_desc("Cumulative Count")
            .draw()?;

        // draw the histogram bars
        for (i, b) in self.buckets.iter().enumerate() {
            let left = if i == 0 {
                0.0
            } else {
                self.buckets[i - 1].upper_bound
            };
            let right = b.upper_bound;
            let height = b.cumulative_count;

            chart.draw_series(std::iter::once(Rectangle::new(
                [(left, 0), (right, height)],
                BLUE.mix(0.75).filled(),
            )))?;
        }

        // draw the step lines
        let mut step_points = vec![(0.0, 0)];
        for b in &self.buckets {
            step_points.push((
                b.upper_bound,
                step_points.last().expect("empty step_points").1,
            ));
            step_points.push((b.upper_bound, b.cumulative_count));
        }
        chart.draw_series(std::iter::once(PathElement::new(
            step_points,
            BLUE.stroke_width(2),
        )))?;

        // draw the quantile lines
        let quantiles = [0.5, 0.9, 0.99];
        let estimates: Vec<(f64, f64)> = quantiles
            .iter()
            .map(|&q| (q, self.buckets.estimate_quantile(q)))
            .collect();
        for (q, val) in estimates {
            chart
                .draw_series(std::iter::once(PathElement::new(
                    vec![(val, 0), (val, max_y)],
                    RED.mix(0.5).stroke_width(2),
                )))?
                .label(format!("p{} ≈ {:.3}", (q * 100.0) as u32, val))
                .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));
        }

        chart
            .configure_series_labels()
            .border_style(&BLACK)
            .background_style(&WHITE.mix(0.8))
            .draw()?;

        Ok(())
    }
}
