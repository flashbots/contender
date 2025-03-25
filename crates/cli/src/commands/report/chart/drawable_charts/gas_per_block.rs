use super::DrawableChart;
use crate::commands::report::util::abbreviate_num;
use alloy::network::AnyRpcBlock;
use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    coord::Shift,
    prelude::{Circle, DrawingArea},
    series::LineSeries,
    style::{
        full_palette::{BLUEGREY_500, GREEN_400},
        FontTransform, IntoTextStyle, ShapeStyle,
    },
};
use std::collections::BTreeMap;

pub struct GasPerBlockChart {
    /// Maps `block_num` to `gas_used`
    gas_used_per_block: BTreeMap<u64, u64>,
}

impl GasPerBlockChart {
    pub fn new(blocks: &[AnyRpcBlock]) -> Self {
        Self {
            gas_used_per_block: blocks
                .iter()
                .map(|block| (block.header.number, block.header.gas_used))
                .collect(),
        }
    }
}

impl DrawableChart for GasPerBlockChart {
    fn define_chart(
        &self,
        root: &DrawingArea<BitMapBackend, Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start_block = self
            .gas_used_per_block
            .keys()
            .min()
            .copied()
            .unwrap_or_default();
        let max_gas_used = self
            .gas_used_per_block
            .values()
            .max()
            .copied()
            .unwrap_or_default();

        let mut chart = ChartBuilder::on(root)
            .margin(15)
            .margin_bottom(25)
            .x_label_area_size(100)
            .y_label_area_size(80)
            .build_cartesian_2d(
                (start_block - 1)..start_block + self.gas_used_per_block.len() as u64,
                0..max_gas_used + (5_000_000 - (max_gas_used % 5_000_000)),
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_desc("Block")
            .x_labels(self.gas_used_per_block.len())
            .x_label_formatter(&|block| format!("            {}", block))
            .x_label_style(
                ("sans-serif", 15)
                    .into_text_style(root)
                    .transform(FontTransform::Rotate90),
            )
            .y_desc("Gas Used")
            .y_labels(25)
            .y_max_light_lines(1)
            .y_label_formatter(&|gas| abbreviate_num(*gas))
            .draw()?;

        // draw line chart
        let chart_data = self
            .gas_used_per_block
            .iter()
            .map(|(block_num, gas_used)| (*block_num, *gas_used));
        chart.draw_series(LineSeries::new(chart_data.to_owned(), &GREEN_400))?;

        // draw dots on line chart
        let mk_dot =
            |c: (u64, u64)| Circle::new(c, 3, Into::<ShapeStyle>::into(BLUEGREY_500).filled());
        chart.draw_series(chart_data.map(|(x, y)| mk_dot((x, y))))?;

        Ok(())
    }
}
