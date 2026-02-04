use crate::block_trace::TxTraceReceipt;
use crate::{Error, Result};
use alloy::hex::ToHexExt;
use alloy::primitives::FixedBytes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::warn;

/// Maximum number of cells to render in the heatmap for performance.
/// With 100 blocks × 100 slots = 10,000 cells max.
const MAX_BLOCKS: usize = 100;
const MAX_SLOTS: usize = 100;

pub struct HeatMapChart {
    updates_per_slot_per_block: BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
}

impl TxTraceReceipt {
    pub fn copy_slot_access_map(
        &self,
        updates_per_slot_per_block: &mut BTreeMap<u64, BTreeMap<FixedBytes<32>, u64>>,
    ) -> Result<()> {
        let block_num = self
            .receipt
            .block_number
            .ok_or(Error::ReceiptMissingBlockNum(self.receipt.transaction_hash))?;
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
            .ok_or(Error::DecodePrestateTraceFrame(trace_frame.to_owned()))?
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
    pub fn new(trace_data: &[TxTraceReceipt]) -> Result<Self> {
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
        let all_blocks = self.get_block_numbers();
        let all_slots = self.get_hex_slots();

        // Condense blocks if there are too many
        let (blocks, block_bucket_size) = if all_blocks.len() > MAX_BLOCKS {
            let bucket_size = all_blocks.len().div_ceil(MAX_BLOCKS);
            let condensed: Vec<u64> = all_blocks
                .chunks(bucket_size)
                .map(|chunk| chunk[0]) // Use first block number as label
                .collect();
            (condensed, bucket_size)
        } else {
            (all_blocks.clone(), 1)
        };

        // Condense slots if there are too many - keep the most active ones
        let slots: Vec<FixedBytes<32>> = if all_slots.len() > MAX_SLOTS {
            // Calculate total access count for each slot across all blocks
            let mut slot_counts: Vec<(FixedBytes<32>, u64)> = all_slots
                .iter()
                .map(|slot| {
                    let total: u64 = self
                        .updates_per_slot_per_block
                        .values()
                        .filter_map(|slot_map| slot_map.get(slot))
                        .sum();
                    (*slot, total)
                })
                .collect();

            // Sort by access count descending and take top MAX_SLOTS
            slot_counts.sort_by_key(|a| a.1);
            slot_counts.truncate(MAX_SLOTS);

            // Sort back by slot value for consistent ordering
            let mut top_slots: Vec<FixedBytes<32>> =
                slot_counts.into_iter().map(|(slot, _)| slot).collect();
            top_slots.sort();
            top_slots
        } else {
            all_slots
        };

        let mut matrix = vec![];
        let mut max_accesses = 0;

        for (i, block_start_idx) in (0..all_blocks.len()).step_by(block_bucket_size).enumerate() {
            let block_end_idx = (block_start_idx + block_bucket_size).min(all_blocks.len());
            let bucket_blocks = &all_blocks[block_start_idx..block_end_idx];

            for (j, slot) in slots.iter().enumerate() {
                // Sum accesses across all blocks in this bucket
                let count: u64 = bucket_blocks
                    .iter()
                    .filter_map(|block| {
                        self.get_slot_map(*block)
                            .and_then(|slot_map| slot_map.get(slot))
                    })
                    .sum();

                if count > max_accesses {
                    max_accesses = count;
                }
                if count > 0 {
                    matrix.push([i as u64, j as u64, count]);
                }
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
