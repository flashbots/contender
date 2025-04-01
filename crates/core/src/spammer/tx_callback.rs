use std::{collections::HashMap, sync::Arc};

use alloy::providers::PendingTransactionConfig;
use tokio::task::JoinHandle;

use crate::{
    eth_engine::{advance_chain, DEFAULT_BLOCK_TIME},
    generator::{types::AnyProvider, NamedTxRequest},
};

use super::tx_actor::TxActorHandle;

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
    fn on_batch_sent(&self) -> Option<JoinHandle<()>>;
}

#[derive(Clone)]
pub struct NilCallback;

pub struct LogCallback {
    pub rpc_provider: Arc<AnyProvider>,
    pub auth_provider: Option<Arc<AnyProvider>>,
    pub send_fcu: bool,
}

impl LogCallback {
    pub fn new(
        rpc_provider: Arc<AnyProvider>,
        auth_provider: Option<Arc<AnyProvider>>,
        send_fcu: bool,
    ) -> Self {
        Self {
            rpc_provider,
            auth_provider,
            send_fcu,
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
        let handle = tokio::task::spawn(async move {
            if let Some(tx_actor) = tx_actor {
                tx_actor
                    .cache_run_tx(*tx_response.tx_hash(), start_timestamp, kind, error)
                    .await
                    .expect("failed to cache run tx");
            }
        });
        Some(handle)
    }
}

impl OnBatchSent for LogCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<()>> {
        if let Some(provider) = &self.auth_provider {
            if !self.send_fcu {
                // maybe do something metrics-related here
                return None;
            }
            let provider = provider.clone();
            return Some(tokio::task::spawn(async move {
                advance_chain(&provider, DEFAULT_BLOCK_TIME)
                    .await
                    .expect("failed to advance chain");
            }));
        }
        None
    }
}

impl OnBatchSent for NilCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<()>> {
        // do nothing
        None
    }
}
