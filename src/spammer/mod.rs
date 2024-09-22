pub mod blockwise;
pub mod timed;
pub mod util;

use alloy::primitives::TxHash;
use tokio::task::JoinHandle;

pub use blockwise::BlockwiseSpammer;
pub use timed::TimedSpammer;

pub trait OnTxSent {
    fn on_tx_sent(&self, tx_hash: TxHash, name: Option<String>) -> Option<JoinHandle<()>>;
}
