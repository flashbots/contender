use std::collections::BTreeMap;

use alloy::rpc::types::Block;
use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    drawing::IntoDrawingArea,
    prelude::Circle,
    series::LineSeries,
    style::{
        full_palette::{BLUEGREY_500, GREEN_400},
        FontTransform, IntoTextStyle, RGBColor, ShapeStyle,
    },
};

use crate::commands::report::util::abbreviate_num;

pub struct GasPerBlockChart {
    /// Maps `block_num` to `gas_used`
    gas_used_per_block: BTreeMap<u64, u128>,
}

impl Default for GasPerBlockChart {
    fn default() -> Self {
        Self::new()
    }
}

impl GasPerBlockChart {
    fn new() -> Self {
        Self {
            gas_used_per_block: Default::default(),
        }
    }

    pub fn build(blocks: &[Block]) -> Self {
        let mut chart = GasPerBlockChart::new();

        for block in blocks {
            chart.set_gas_used(block.header.number, block.header.gas_used);
        }

        chart
    }

    fn set_gas_used(&mut self, block_num: u64, gas_used: u128) {
        self.gas_used_per_block.insert(block_num, gas_used);
    }

    /// Draws the chart and saves the image to `filepath`.
    ///
    /// TODO: DRY duplicate code in other chart modules. This could be a trait-defined generic method.
    pub fn draw(&self, filepath: impl AsRef<str>) -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new(filepath.as_ref(), (1024, 768)).into_drawing_area();
        root.fill(&RGBColor(255, 255, 255))?;

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

        let mut chart = ChartBuilder::on(&root)
            .margin(15)
            .margin_bottom(25)
            .x_label_area_size(100)
            .y_label_area_size(80)
            .build_cartesian_2d(
                (start_block - 1)..start_block + self.gas_used_per_block.len() as u64,
                0..max_gas_used,
            )?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .x_desc("Block")
            .x_labels(self.gas_used_per_block.len())
            .x_label_formatter(&|block| format!("            {}", block))
            .x_label_style(
                ("sans-serif", 15)
                    .into_text_style(&root)
                    .transform(FontTransform::Rotate90),
            )
            .y_desc("Gas Used")
            .y_labels(25)
            .y_max_light_lines(1)
            .y_label_formatter(&|gas| abbreviate_num(*gas as u64))
            .draw()?;

        // draw line chart
        let chart_data = self
            .gas_used_per_block
            .iter()
            .map(|(block_num, gas_used)| (*block_num, *gas_used));
        chart.draw_series(LineSeries::new(chart_data.to_owned(), &GREEN_400))?;

        // draw dots on line chart
        let mk_dot =
            |c: (u64, u128)| Circle::new(c, 3, Into::<ShapeStyle>::into(BLUEGREY_500).filled());
        chart.draw_series(chart_data.map(|(x, y)| mk_dot((x, y))))?;

        root.present()?;
        println!("saved chart to {}", filepath.as_ref());

        Ok(())
    }
}
