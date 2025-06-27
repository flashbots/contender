use crate::block_trace::TxTraceReceipt;
use alloy::hex::ToHexExt;
use alloy::primitives::FixedBytes;
use contender_core::error::ContenderError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::warn;

pub struct HeatMapChart {
    updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
}

impl TxTraceReceipt {
    pub fn copy_slot_access_map(
        &self,
        updates_per_slot_per_block: &mut BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
    ) -> Result<(), ContenderError> {
        let block_num = self
            .receipt
            .block_number
            .ok_or(ContenderError::GenericError(
                "Block number not found in receipt.",
                "".to_string(),
            ))?;
        let trace_frame = self.trace.to_owned().try_into_pre_state_frame().ok();
        // If the trace frame is None, it means that the preState trace was not found.
        // This can happen if the target node does not support preState traces.
        if trace_frame.is_none() {
            // Log a warning and return early
            warn!(
                "No preState trace frame found for block number {}. This may indicate that the target node does not support preState traces.",
                block_num
            );
            return Ok(());
        }
        let trace_frame = trace_frame.expect("trace frame should be Some");
        let account_map = &trace_frame
            .as_default()
            .ok_or(ContenderError::GenericError(
                "failed to decode PreStateMode",
                format!("{trace_frame:?}"),
            ))?
            .0;

        // "for each account in this transaction trace"
        for (_key, update) in account_map.iter() {
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
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HeatmapData {
    pub blocks: Vec<u64>,
    pub slots: Vec<String>,
    pub matrix: Vec<[u64; 3]>,
    pub max_accesses: u64,
}

/// Represents data as a mapping of block_num => slot => count.
impl HeatMapChart {
    pub fn new(trace_data: &[TxTraceReceipt]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>> =
            Default::default();

        for t in trace_data {
            t.copy_slot_access_map(&mut updates_per_slot_per_block)?;
        }

        if updates_per_slot_per_block.is_empty() {
            warn!("No trace data was collected. If transactions from the specified run landed, your target node may not support geth-style preState traces");
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

    pub fn echart_data(&self) -> HeatmapData {
        let blocks = self.get_block_numbers();
        let slots = self.get_hex_slots();
        let mut matrix = vec![];
        let mut max_accesses = 0;
        for (i, block) in blocks.iter().enumerate() {
            for (j, slot) in slots.iter().enumerate() {
                let count = self
                    .get_slot_map(*block)
                    .and_then(|slot_map| slot_map.get(slot))
                    .cloned()
                    .unwrap_or(0);
                if count > max_accesses {
                    max_accesses = count;
                }
                matrix.push([i as u64, j as u64, count]);
            }
        }
        HeatmapData {
            blocks,
            slots: slots.iter().map(|h| h.encode_hex()).collect(),
            matrix,
            max_accesses,
        }
    }

    /// returns all slots in the heatmap
    fn get_hex_slots(&self) -> Vec<FixedBytes<32>> {
        let mut slots = self
            .updates_per_slot_per_block
            .values()
            .flat_map(|slot_map| slot_map.keys())
            // filter out duplicates
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .map(|h| h.to_owned())
            .collect::<Vec<_>>();
        slots.sort();
        slots
    }
}
