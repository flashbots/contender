use alloy::network::AnyRpcBlock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub struct GasPerBlockChart {
    /// Maps `block_num` to `gas_used`
    gas_used_per_block: BTreeMap<u64, u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GasPerBlockData {
    pub blocks: Vec<u64>,
    pub gas_used: Vec<u64>,
    pub max_gas_used: u64,
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

    fn block_nums(&self) -> Vec<u64> {
        self.gas_used_per_block.keys().cloned().collect()
    }

    fn gas_values(&self) -> Vec<u64> {
        self.gas_used_per_block.values().cloned().collect()
    }

    pub fn echart_data(&self) -> GasPerBlockData {
        GasPerBlockData {
            blocks: self.block_nums(),
            gas_used: self.gas_values(),
            max_gas_used: self
                .gas_used_per_block
                .values()
                .max()
                .copied()
                .unwrap_or_default(),
        }
    }
}
