use alloy::primitives::Bytes;
use serde::{Deserialize, Serialize};

use crate::bundle::Bundle;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RevertProtectBundle {
    #[serde(rename = "txs")]
    transactions: Vec<Bytes>,
    block_number_min: Option<u64>,
    block_number_max: Option<u64>,
}

impl RevertProtectBundle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_txs(self, transactions: Vec<Bytes>) -> Self {
        Self {
            transactions,
            block_number_min: self.block_number_min,
            block_number_max: self.block_number_max,
        }
    }

    pub fn with_block_min(self, min_block: u64) -> Self {
        Self {
            transactions: self.transactions,
            block_number_min: Some(min_block),
            block_number_max: self.block_number_max,
        }
    }

    pub fn with_block_max(self, max_block: u64) -> Self {
        Self {
            transactions: self.transactions,
            block_number_min: self.block_number_min,
            block_number_max: Some(max_block),
        }
    }

    pub fn prepare(self) -> Bundle {
        Bundle::Revertable(self)
    }
}
