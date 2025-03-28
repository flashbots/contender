use super::DrawableChart;
use crate::commands::report::block_trace::TxTraceReceipt;
use alloy::primitives::FixedBytes;
use plotters::prelude::*;
use std::collections::BTreeMap;

pub struct HeatMapChart {
    updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
}

/// Represents data as a mapping of block_num => slot => count.
impl HeatMapChart {
    pub fn new(trace_data: &[TxTraceReceipt]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>> =
            Default::default();

        for t in trace_data {
            let block_num = t
                .receipt
                .block_number
                .expect("block number not found in receipt");

            let trace_frame = t.trace.to_owned().try_into_pre_state_frame();
            if let Err(e) = trace_frame {
                println!("failed to decode frame (preState mode): {:?}", e);
                continue;
            }
            let trace_frame = trace_frame.expect("failed to decode frame (preState mode)");
            let account_map = &trace_frame
                .as_default()
                .expect("failed to decode PreStateMode")
                .0;

            // "for each account in this transaction trace"
            for key in account_map.keys() {
                let update = account_map
                    .get(key)
                    .expect("invalid key; this should never happen");
                // for every storage slot in this frame, increment the count for the slot at this block number
                update.storage.iter().for_each(|(slot, _)| {
                    if let Some(slot_map) = updates_per_slot_per_block.get_mut(&block_num) {
                        let value = slot_map.get(slot).map(|v| v + 1).unwrap_or(1);
                        slot_map.insert(*slot, value);
                    } else {
                        let mut slot_map = BTreeMap::new();
                        slot_map.insert(*slot, 1);
                        updates_per_slot_per_block.insert(block_num, slot_map);
                    }
                });
            }
        }

        if updates_per_slot_per_block.is_empty() {
            return Err("No trace data was collected. If transactions from the specified run landed, your target node may not support geth-style preState traces".into());
        }

        Ok(Self {
            updates_per_slot_per_block,
        })
    }

    fn get_block_numbers(&self) -> Vec<u64> {
        self.updates_per_slot_per_block.keys().cloned().collect()
    }

    fn get_slot_map(&self, block_num: u64) -> Option<&BTreeMap<FixedBytes<32>, u64>> {
        self.updates_per_slot_per_block.get(&block_num)
    }

    fn get_num_blocks(&self) -> usize {
        self.updates_per_slot_per_block.len()
    }

    fn get_num_slots(&self) -> usize {
        self.updates_per_slot_per_block
            .values()
            .flat_map(|slot_map| slot_map.keys())
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    fn get_matrix(&self) -> Vec<Vec<u64>> {
        let mut matrix = vec![vec![0; self.get_num_slots()]; self.get_num_blocks()];
        let block_nums = self.get_block_numbers();

        // map slots to matrix indices
        let mut slot_indices = BTreeMap::new();
        let mut all_keys = vec![];
        for slot_map in self.updates_per_slot_per_block.values() {
            // collect keys (slots) from all slot maps
            all_keys.extend_from_slice(slot_map.keys().collect::<Vec<_>>().as_slice());
        }
        all_keys.sort();
        let mut slot_counter = 0;
        for slot in all_keys {
            if !slot_indices.contains_key(slot) {
                slot_indices.insert(*slot, slot_counter);
                slot_counter += 1;
            }
        }

        // build matrix
        for (i, bn) in block_nums.iter().enumerate() {
            let slot_map = self
                .get_slot_map(*bn)
                .expect("invalid key; this should never happen");
            for (slot, count) in slot_map {
                let j = *slot_indices.get(slot).expect("slot index not found");
                matrix[i][j] = *count;
            }
        }
        matrix
    }

    /// returns all slots in the heatmap as a list of hex strings
    fn get_hex_slots(&self) -> Vec<String> {
        let mut slots = self
            .updates_per_slot_per_block
            .values()
            .flat_map(|slot_map| slot_map.keys())
            // filter out duplicates
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        slots.sort();
        slots.iter().map(|s| format!("{:?}", s)).collect()
    }
}

impl DrawableChart for HeatMapChart {
    fn define_chart(
        &self,
        root: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (chart_area, legend_area) = root.split_horizontally(900);
        let legend_area = legend_area.margin(40, 40, 10, 10);

        let block_nums = self.get_block_numbers();
        let slot_names = self.get_hex_slots();
        let matrix = self.get_matrix();

        let x_size = matrix.len();
        let y_size = matrix[0].len();
        let max_incidence = matrix
            .iter()
            .map(|r| r.iter().max().expect("empty row"))
            .max()
            .expect("empty matrix");
        let mut chart = ChartBuilder::on(&chart_area)
            .margin(20)
            .x_label_area_size(80)
            .y_label_area_size(120)
            .build_cartesian_2d(0..x_size, 0..y_size)?;

        chart
            .configure_mesh()
            .x_desc("Block")
            .x_labels(20)
            .x_label_formatter(&|i| {
                if *i == block_nums.len() {
                    return String::default();
                }
                let block_num = block_nums.get(*i).unwrap();
                format!("            {}", block_num)
            })
            .y_desc("Storage Slot")
            .y_label_formatter(&|i| {
                if *i == 0 {
                    return String::default();
                }
                let slot = slot_names
                    .get(*i - 1)
                    .map(|n| n.to_owned())
                    .unwrap_or_default();
                // truncate slot for display
                format!("{}...{}", &slot[..8], &slot[60..])
            })
            .y_labels(64)
            .y_label_style(("monospace", 10))
            .x_label_offset(24)
            .disable_x_mesh()
            .disable_y_mesh()
            .x_label_style(
                ("sans-serif", 15)
                    .into_text_style(&chart_area)
                    .transform(FontTransform::Rotate90),
            )
            .draw()?;

        chart
            .draw_series(
                matrix
                    .iter()
                    .zip(0..)
                    .flat_map(|(l, x)| l.iter().zip(0..).map(move |(v, y)| (x, y, v)))
                    .map(|(x, y, v)| {
                        let brightness = (v * 255 / max_incidence) as u8;
                        let (r, g, b) = rgb_gradient(brightness);
                        Rectangle::new([(x, y), (x + 1, y + 1)], RGBColor(r, g, b).filled())
                    }),
            )?
            .label("heatmap");

        // Draw vertical color gradient in the legend area
        let legend_height = 700;

        for i in 0..=*max_incidence {
            let brightness = (i * 255 / max_incidence) as u8;
            let (r, g, b) = rgb_gradient(brightness);
            let y_start = legend_height - (i * (legend_height / max_incidence));
            let chunk_size = legend_height / max_incidence;
            let y_end = y_start.max(chunk_size) - chunk_size;

            legend_area.draw(&Rectangle::new(
                [(50, y_start as i32), (80, y_end as i32)], // Small vertical bar
                RGBColor(r, g, b).filled(),
            ))?;
        }

        // Draw legend labels
        legend_area.draw(&Text::new(
            max_incidence.to_string(),
            (90, 0),
            ("sans-serif", 15),
        ))?;
        legend_area.draw(&Text::new(
            "0",
            (90, legend_height as i32),
            ("sans-serif", 15),
        ))?;

        // draw legend title
        legend_area.draw(&Text::new(
            "# Slot Updates",
            (40, legend_height as i32 / 2),
            ("sans-serif", 15)
                .into_font()
                .transform(FontTransform::Rotate90),
        ))?;

        Ok(())
    }
}

fn rgb_gradient(value: u8) -> (u8, u8, u8) {
    match value {
        0..=85 => (value * 3, 0, 0),                // Black to Red (R increases)
        86..=170 => (255, (value - 85) * 3, 0),     // Red to Yellow (G increases)
        171..=255 => (255, 255, (value - 170) * 3), // Yellow to White (B increases)
    }
}
