use super::tx_actor::{CacheTx, TxActorHandle};
use crate::generator::{types::AnyProvider, NamedTxRequest};
use alloy::providers::PendingTransactionConfig;
use contender_engine_provider::{AdvanceChain, DEFAULT_BLOCK_TIME};
use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;
use tracing::debug;

pub trait OnTxSent<K = String, V = String>
where
    K: Eq + std::hash::Hash + AsRef<str>,
    V: AsRef<str>,
{
    fn on_tx_sent(
        &self,
        tx_response: PendingTransactionConfig,
        req: &NamedTxRequest,
        extra: Option<HashMap<K, V>>,
        tx_handler: Option<Arc<TxActorHandle>>,
    ) -> Option<JoinHandle<()>>;
}

pub trait OnBatchSent {
    fn on_batch_sent(&self) -> Option<JoinHandle<Result<(), String>>>;
}

pub trait SpamCallback: OnTxSent + OnBatchSent + Send + Sync {}

impl<T: OnTxSent + OnBatchSent + Sized + Send + Sync> SpamCallback for T {}

#[derive(Clone)]
pub struct NilCallback;

pub struct LogCallback {
    pub rpc_provider: Arc<AnyProvider>,
    pub auth_provider: Option<Arc<dyn AdvanceChain + Send + Sync + 'static>>,
    pub send_fcu: bool,
    pub cancel_token: tokio_util::sync::CancellationToken,
}

impl LogCallback {
    pub fn new(
        rpc_provider: Arc<AnyProvider>,
        auth_provider: Option<Arc<dyn AdvanceChain + Send + Sync + 'static>>,
        send_fcu: bool,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            rpc_provider,
            auth_provider,
            send_fcu,
            cancel_token,
        }
    }
}

impl OnTxSent for NilCallback {
    fn on_tx_sent(
        &self,
        _tx_res: PendingTransactionConfig,
        _req: &NamedTxRequest,
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
        _req: &NamedTxRequest,
        extra: Option<HashMap<String, String>>,
        tx_actor: Option<Arc<TxActorHandle>>,
    ) -> Option<JoinHandle<()>> {
        let start_timestamp = extra
            .as_ref()
            .and_then(|e| e.get("start_timestamp").map(|t| t.parse::<u64>()))?
            .unwrap_or(0);
        let kind = extra
            .as_ref()
            .and_then(|e| e.get("kind").map(|k| k.to_owned()));
        let error = extra
            .as_ref()
            .and_then(|e| e.get("error").map(|e| e.to_owned()));
        let cancel_token = self.cancel_token.clone();
        let handle = tokio::task::spawn(async move {
            if let Some(tx_actor) = tx_actor {
                let tx = CacheTx {
                    tx_hash: *tx_response.tx_hash(),
                    start_timestamp,
                    kind,
                    error,
                };
                tokio::select! {
                    _ = cancel_token.cancelled() => {}
                    _ = tx_actor.cache_run_tx(tx) => {}
                };
            }
        });
        Some(handle)
    }
}

impl OnBatchSent for LogCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<Result<(), String>>> {
        debug!("on_batch_sent called");
        if !self.send_fcu {
            // maybe do something metrics-related here
            return None;
        }
        if let Some(provider) = &self.auth_provider {
            let provider = provider.clone();
            return Some(tokio::task::spawn(async move {
                let res = provider.advance_chain(DEFAULT_BLOCK_TIME).await;
                res.map_err(|e| e.to_string())
            }));
        }
        None
    }
}

impl OnBatchSent for NilCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<Result<(), String>>> {
        // do nothing
        None
    }
}
