use alloy::primitives::Bytes;
use serde::{Deserialize, Serialize};

use crate::bundle::Bundle;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevertProtectBundle {
    transaction: Bytes,
    block_number_min: Option<u64>,
    block_number_max: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct RevertProtectBundleRequest {
    pub txs: Vec<Bytes>,
    pub block_number_min: Option<u64>,
    pub block_number_max: Option<u64>,
}

impl RevertProtectBundleRequest {
    pub fn new() -> Self {
        Self {
            txs: vec![],
            block_number_min: None,
            block_number_max: None,
        }
    }

    pub fn with_txs(self, txs: Vec<Bytes>) -> Self {
        Self {
            txs,
            block_number_min: self.block_number_min,
            block_number_max: self.block_number_max,
        }
    }

    pub fn with_block_min(self, min_block: u64) -> Self {
        Self {
            txs: self.txs,
            block_number_min: Some(min_block),
            block_number_max: self.block_number_max,
        }
    }

    pub fn with_block_max(self, max_block: u64) -> Self {
        Self {
            txs: self.txs,
            block_number_min: self.block_number_min,
            block_number_max: Some(max_block),
        }
    }

    pub fn prepare(self) -> Bundle {
        Bundle::Revertable(self)
    }
}

impl From<&RevertProtectBundleRequest> for RevertProtectBundle {
    fn from(value: &RevertProtectBundleRequest) -> Self {
        value.to_owned().into()
    }
}

impl From<RevertProtectBundleRequest> for RevertProtectBundle {
    fn from(value: RevertProtectBundleRequest) -> Self {
        let RevertProtectBundleRequest {
            txs,
            block_number_min,
            block_number_max,
        } = value;

        let mut buf = vec![];
        alloy::rlp::encode_list::<Bytes, Bytes>(&txs, &mut buf);
        let tx_bytes = Bytes::from_iter(&buf);

        Self {
            transaction: tx_bytes,
            block_number_min,
            block_number_max,
        }
    }
}
