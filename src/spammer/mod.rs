pub mod blockwise;
pub mod timed;
pub mod tx_actor;
pub mod util;

use crate::generator::NamedTxRequest;
use alloy::providers::PendingTransactionConfig;
use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;
use tx_actor::TxActorHandle;

pub use blockwise::BlockwiseSpammer;
pub use timed::TimedSpammer;

pub trait OnTxSent<K = String, V = String>
where
    K: Eq + std::hash::Hash + AsRef<str>,
    V: AsRef<str>,
{
    fn on_tx_sent(
        &self,
        tx_response: PendingTransactionConfig,
        req: NamedTxRequest,
        extra: Option<HashMap<K, V>>,
        tx_handler: Option<Arc<TxActorHandle>>,
    ) -> Option<JoinHandle<()>>;
}
