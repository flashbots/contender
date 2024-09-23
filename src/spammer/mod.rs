pub mod blockwise;
pub mod timed;
pub mod util;

use std::collections::HashMap;

use alloy::primitives::TxHash;
use tokio::task::JoinHandle;

pub use blockwise::BlockwiseSpammer;
pub use timed::TimedSpammer;

use crate::generator::NamedTxRequest;

pub trait OnTxSent<K = String>
where
    K: Eq + std::hash::Hash + AsRef<str>,
{
    fn on_tx_sent(
        &self,
        tx_hash: TxHash,
        req: NamedTxRequest,
        extra: Option<HashMap<K, String>>,
    ) -> Option<JoinHandle<()>>;
}
