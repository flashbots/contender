use alloy::primitives::FixedBytes;

use crate::commands::report::TxTraceReceipt;
use std::collections::BTreeMap;

pub struct HeatMapBuilder;

pub struct HeatMap {
    updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
}

impl Default for HeatMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents data as a mapping of block_num => slot => count.
impl HeatMap {
    fn new() -> Self {
        Self {
            updates_per_slot_per_block: Default::default(),
        }
    }

    fn add_update(&mut self, block_num: u64, slot: FixedBytes<32>) {
        if let Some(slot_map) = self.updates_per_slot_per_block.get_mut(&block_num) {
            let value = slot_map.get(&slot).map(|v| v + 1).unwrap_or(1);
            slot_map.insert(slot, value);
            return;
        } else {
            let mut slot_map = BTreeMap::new();
            slot_map.insert(slot, 1);
            self.updates_per_slot_per_block.insert(block_num, slot_map);
        }
    }

    fn get_block_numbers(&self) -> Vec<u64> {
        self.updates_per_slot_per_block.keys().cloned().collect()
    }

    fn get_slot_map(&self, block_num: u64) -> Option<&BTreeMap<FixedBytes<32>, u64>> {
        self.updates_per_slot_per_block.get(&block_num)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("saving heatmap");
        Ok(())
    }
}

impl HeatMapBuilder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(
        &self,
        trace_data: &[TxTraceReceipt],
    ) -> Result<HeatMap, Box<dyn std::error::Error>> {
        let mut heatmap = HeatMap::new();

        for t in trace_data {
            let block_num = t
                .receipt
                .block_number
                .expect("block number not found in receipt");
            let trace_frame = t
                .trace
                .to_owned()
                .try_into_pre_state_frame()
                .expect("failed to decode prestate frame");
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
                    heatmap.add_update(block_num, *slot);
                });
            }
        }

        let block_nums = heatmap.get_block_numbers();
        for bn in block_nums {
            let slot_map = heatmap
                .get_slot_map(bn)
                .expect("invalid key; this should never happen");
            println!("BLOCK: {}", bn);
            for (slot, count) in slot_map {
                println!("  SLOT: {} COUNT: {}", slot, count);
            }
        }

        Ok(heatmap)
    }
}
