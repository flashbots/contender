use super::tx_actor::{CacheTx, TxActorHandle};
use crate::{
    error::ContenderError,
    generator::{types::AnyProvider, NamedTxRequest},
};
use alloy::providers::PendingTransactionConfig;
use contender_engine_provider::{AdvanceChain, DEFAULT_BLOCK_TIME};
use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
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
        extra: RuntimeTxInfo,
        tx_handlers: Option<HashMap<String, Arc<TxActorHandle>>>,
    ) -> Option<JoinHandle<crate::Result<()>>>;
}

pub trait OnBatchSent {
    fn on_batch_sent(&self) -> Option<JoinHandle<crate::Result<()>>>;
}

pub trait SpamCallback: OnTxSent + OnBatchSent + Send + Sync {}

#[derive(Clone, Debug)]
pub struct RuntimeTxInfo {
    start_timestamp_ms: u128,
    kind: Option<String>,
    error: Option<String>,
}

impl RuntimeTxInfo {
    pub fn new(start_timestamp_ms: u128, kind: Option<String>, error: Option<String>) -> Self {
        Self {
            start_timestamp_ms,
            kind,
            error,
        }
    }

    pub fn with_kind(mut self, kind: String) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_start_timestamp(mut self, start_timestamp_ms: u128) -> Self {
        self.start_timestamp_ms = start_timestamp_ms;
        self
    }

    pub fn start_timestamp_ms(&self) -> u128 {
        self.start_timestamp_ms
    }

    pub fn kind(&self) -> Option<&String> {
        self.kind.as_ref()
    }

    pub fn error(&self) -> Option<&String> {
        self.error.as_ref()
    }
}

impl Default for RuntimeTxInfo {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            start_timestamp_ms: now,
            kind: None,
            error: None,
        }
    }
}

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
    pub fn new(rpc_provider: Arc<AnyProvider>) -> Self {
        Self {
            rpc_provider,
            auth_provider: None,
            send_fcu: false,
            cancel_token: CancellationToken::default(),
        }
    }
    pub fn auth_provider(
        mut self,
        provider: Arc<dyn AdvanceChain + Send + Sync + 'static>,
    ) -> Self {
        self.auth_provider = Some(provider);
        self
    }
    pub fn send_fcu(mut self, send_fcu: bool) -> Self {
        self.send_fcu = send_fcu;
        self
    }
    pub fn cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = token;
        self
    }
}

impl OnTxSent for NilCallback {
    fn on_tx_sent(
        &self,
        _tx_res: PendingTransactionConfig,
        _req: &NamedTxRequest,
        _extra: RuntimeTxInfo,
        _tx_handlers: Option<HashMap<String, Arc<TxActorHandle>>>,
    ) -> Option<JoinHandle<crate::Result<()>>> {
        // do nothing
        None
    }
}

impl OnTxSent for LogCallback {
    fn on_tx_sent(
        &self,
        tx_response: PendingTransactionConfig,
        _req: &NamedTxRequest,
        extra: RuntimeTxInfo,
        tx_actors: Option<HashMap<String, Arc<TxActorHandle>>>,
    ) -> Option<JoinHandle<crate::Result<()>>> {
        let cancel_token = self.cancel_token.clone();
        let handle = tokio::task::spawn(async move {
            if let Some(tx_actors) = tx_actors {
                let tx_actor = tx_actors["default"].clone();
                let tx = CacheTx {
                    tx_hash: *tx_response.tx_hash(),
                    start_timestamp_ms: extra.start_timestamp_ms,
                    kind: extra.kind,
                    error: extra.error,
                };
                tokio::select! {
                    _ = cancel_token.cancelled() => {}
                    _ = tx_actor.cache_run_tx(tx) => {}
                };
            }
            Ok(())
        });
        Some(handle)
    }
}

impl OnBatchSent for LogCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<crate::Result<()>>> {
        debug!("on_batch_sent called");
        if !self.send_fcu {
            // maybe do something metrics-related here
            return None;
        }
        if let Some(provider) = &self.auth_provider {
            let provider = provider.clone();
            return Some(tokio::task::spawn(async move {
                provider
                    .advance_chain(DEFAULT_BLOCK_TIME)
                    .await
                    .map_err(ContenderError::from)
            }));
        }
        None
    }
}

impl OnBatchSent for NilCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<crate::Result<()>>> {
        // do nothing
        None
    }
}
