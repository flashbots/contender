use alloy::primitives::Bytes;
use serde::{Deserialize, Serialize};

use crate::bundle_provider::bundle::TypedBundle;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RevertProtectBundle {
    #[serde(rename = "txs")]
    transaction: Vec<Bytes>,
    block_number_max: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct RevertProtectBundleRequest {
    pub txs: Vec<Bytes>,
    pub block_number_max: Option<u64>,
}

impl RevertProtectBundleRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_txs(self, txs: Vec<Bytes>) -> Self {
        Self {
            txs,
            block_number_max: self.block_number_max,
        }
    }

    pub fn with_block_max(self, max_block: u64) -> Self {
        Self {
            txs: self.txs,
            block_number_max: Some(max_block),
        }
    }

    pub fn prepare(self) -> TypedBundle {
        TypedBundle::RevertProtected(self)
    }
}

impl AsRef<RevertProtectBundleRequest> for RevertProtectBundleRequest {
    fn as_ref(&self) -> &RevertProtectBundleRequest {
        self
    }
}

impl<T: AsRef<RevertProtectBundleRequest>> From<T> for RevertProtectBundle {
    fn from(req: T) -> Self {
        let RevertProtectBundleRequest {
            txs,
            block_number_max,
        } = req.as_ref();

        if txs.is_empty() {
            panic!("RevertProtectBundleRequest must have at least one transaction");
        }
        // temporary until revert-protect bundles support multiple transactions
        if txs.len() > 1 {
            panic!("RevertProtectBundleRequest can only contain one transaction");
        }

        Self {
            transaction: txs.to_owned(),
            block_number_max: block_number_max.to_owned(),
        }
    }
}

/// Temporary until revert-protect bundles support multiple transactions.
/// Once that is supported, this trait can be removed and `RevertProtectBundleRequest::into::<RevertProtectBundle>()` can be used instead.
pub trait BundlesFromRequest {
    fn to_bundles(&self) -> Vec<RevertProtectBundle>;
}

impl BundlesFromRequest for RevertProtectBundleRequest {
    /// Converts a RevertProtectBundleRequest into Vec<RevertProtectBundle>.
    fn to_bundles(&self) -> Vec<RevertProtectBundle> {
        self.txs
            .iter()
            .map(|tx| RevertProtectBundle {
                transaction: vec![tx.to_owned()],
                block_number_max: self.block_number_max,
            })
            .collect()
    }
}
