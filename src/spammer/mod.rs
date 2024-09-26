pub mod blockwise;
pub mod timed;
pub mod tx_actor;
pub mod util;

use crate::generator::{types::RpcProvider, NamedTxRequest};
use alloy::providers::{PendingTransactionBuilder, PendingTransactionConfig};
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

pub struct NilCallback;

impl NilCallback {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct LogCallback {
    pub rpc_provider: Arc<RpcProvider>,
}

impl LogCallback {
    pub fn new(rpc_provider: Arc<RpcProvider>) -> Self {
        Self { rpc_provider }
    }
}

impl OnTxSent for NilCallback {
    fn on_tx_sent(
        &self,
        _tx_res: PendingTransactionConfig,
        _req: NamedTxRequest,
        _extra: Option<HashMap<String, String>>,
        _tx_handler: Option<Arc<TxActorHandle>>,
    ) -> Option<JoinHandle<()>> {
        // do nothing
        None
    }
}

impl OnTxSent for LogCallback {
    fn on_tx_sent(
        &self,
        tx_response: PendingTransactionConfig,
        _req: NamedTxRequest,
        extra: Option<HashMap<String, String>>,
        tx_actor: Option<Arc<TxActorHandle>>,
    ) -> Option<JoinHandle<()>> {
        let rpc = self.rpc_provider.clone();
        let start_timestamp = extra
            .as_ref()
            .map(|e| e.get("start_timestamp").map(|t| t.parse::<usize>()))
            .flatten()?
            .unwrap_or(0);
        let handle = tokio::task::spawn(async move {
            let res = PendingTransactionBuilder::from_config(&rpc, tx_response);
            let tx_hash = res.tx_hash();
            if let Some(tx_actor) = tx_actor {
                tx_actor
                    .cache_run_tx(*tx_hash, start_timestamp)
                    .await
                    .expect("failed to cache run tx");
            }
        });
        Some(handle)
    }
}
