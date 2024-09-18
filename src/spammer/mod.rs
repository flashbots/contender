pub mod blockwise;
pub mod timed;
pub mod util;

use alloy::primitives::TxHash;
use tokio::task::JoinHandle;

pub trait SpamCallback {
    fn on_tx_sent(&self, tx_hash: TxHash, name: Option<String>) -> Option<JoinHandle<()>>;
}
