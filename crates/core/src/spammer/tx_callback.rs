use super::tx_actor::{CacheTx, TxActorHandle};
use crate::{
    generator::{types::AnyProvider, NamedTxRequest},
    spammer::{CallbackError, CallbackResult},
};
use alloy::providers::PendingTransactionConfig;
use contender_engine_provider::{ControlChain, DEFAULT_BLOCK_TIME};
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
    ) -> Option<JoinHandle<CallbackResult<()>>>;
}

pub trait OnBatchSent {
    fn on_batch_sent(&self) -> Option<JoinHandle<CallbackResult<()>>>;
}

pub trait SpamCallback: OnTxSent + OnBatchSent + Send + Sync {}

#[derive(Clone, Debug)]
pub struct RuntimeTxInfo {
    pub start_timestamp_ms: u128,
    pub end_timestamp_ms: Option<u128>,
    pub kind: Option<String>,
    pub error: Option<String>,
}

impl RuntimeTxInfo {
    /// Capture the current system time as the start timestamp.
    /// Call this immediately before the network send to ensure accurate latency measurement.
    pub fn now() -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            start_timestamp_ms: ts,
            end_timestamp_ms: None,
            kind: None,
            error: None,
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

    pub fn with_end_timestamp(mut self, end_timestamp_ms: u128) -> Self {
        self.end_timestamp_ms = Some(end_timestamp_ms);
        self
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
            end_timestamp_ms: None,
            kind: None,
            error: None,
        }
    }
}

impl<T: OnTxSent + OnBatchSent + Sized + Send + Sync> SpamCallback for T {}

#[derive(Clone)]
pub struct NilCallback;

#[derive(Clone)]
pub struct LogCallback {
    pub rpc_provider: Arc<AnyProvider>,
    pub auth_provider: Option<Arc<dyn ControlChain + Send + Sync + 'static>>,
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
        provider: Arc<dyn ControlChain + Send + Sync + 'static>,
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

    /// Wrap this `LogCallback` with a user-provided callback. The returned
    /// [`CombinedCallback`] runs the `LogCallback`'s tx-caching and FCU logic
    /// alongside `custom`, so custom callbacks automatically keep the DB in
    /// sync with `contender report` without having to duplicate
    /// `LogCallback`'s internals.
    pub fn with_callback<C>(self, custom: C) -> CombinedCallback<LogCallback, C> {
        CombinedCallback::new(self, custom)
    }
}

/// Runs two callbacks in tandem on every `on_tx_sent` / `on_batch_sent`, joining
/// any spawned tasks into a single handle so the spam loop still sees one unit
/// of work. The typical pairing is a [`LogCallback`] as `base` plus a
/// user-provided callback as `custom`, which lets custom callbacks inherit the
/// DB-persistence and FCU behavior that `contender report` depends on.
#[derive(Clone)]
pub struct CombinedCallback<A, B> {
    pub base: A,
    pub custom: B,
}

impl<A, B> CombinedCallback<A, B> {
    pub fn new(base: A, custom: B) -> Self {
        Self { base, custom }
    }
}

impl<A, B> OnTxSent for CombinedCallback<A, B>
where
    A: OnTxSent + Send + Sync + 'static,
    B: OnTxSent + Send + Sync + 'static,
{
    fn on_tx_sent(
        &self,
        tx_response: PendingTransactionConfig,
        req: &NamedTxRequest,
        extra: RuntimeTxInfo,
        tx_handlers: Option<HashMap<String, Arc<TxActorHandle>>>,
    ) -> Option<JoinHandle<CallbackResult<()>>> {
        let base_handle =
            self.base
                .on_tx_sent(tx_response.clone(), req, extra.clone(), tx_handlers.clone());
        let custom_handle = self.custom.on_tx_sent(tx_response, req, extra, tx_handlers);
        join_callback_handles(base_handle, custom_handle)
    }
}

impl<A, B> OnBatchSent for CombinedCallback<A, B>
where
    A: OnBatchSent + Send + Sync + 'static,
    B: OnBatchSent + Send + Sync + 'static,
{
    fn on_batch_sent(&self) -> Option<JoinHandle<CallbackResult<()>>> {
        let base_handle = self.base.on_batch_sent();
        let custom_handle = self.custom.on_batch_sent();
        join_callback_handles(base_handle, custom_handle)
    }
}

fn join_callback_handles(
    a: Option<JoinHandle<CallbackResult<()>>>,
    b: Option<JoinHandle<CallbackResult<()>>>,
) -> Option<JoinHandle<CallbackResult<()>>> {
    match (a, b) {
        (None, None) => None,
        (Some(h), None) | (None, Some(h)) => Some(h),
        (Some(a), Some(b)) => Some(tokio::task::spawn(async move {
            let (ra, rb) = tokio::join!(a, b);
            ra.map_err(CallbackError::Join)??;
            rb.map_err(CallbackError::Join)??;
            Ok(())
        })),
    }
}

impl OnTxSent for NilCallback {
    fn on_tx_sent(
        &self,
        _tx_res: PendingTransactionConfig,
        _req: &NamedTxRequest,
        _extra: RuntimeTxInfo,
        _tx_handlers: Option<HashMap<String, Arc<TxActorHandle>>>,
    ) -> Option<JoinHandle<CallbackResult<()>>> {
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
    ) -> Option<JoinHandle<CallbackResult<()>>> {
        let cancel_token = self.cancel_token.clone();
        let handle = crate::spawn_with_session(async move {
            if let Some(tx_actors) = tx_actors {
                let tx_actor = tx_actors["default"].clone();
                let tx = CacheTx {
                    tx_hash: *tx_response.tx_hash(),
                    start_timestamp_ms: extra.start_timestamp_ms,
                    end_timestamp_ms: extra.end_timestamp_ms,
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
    fn on_batch_sent(&self) -> Option<JoinHandle<CallbackResult<()>>> {
        debug!("on_batch_sent called");
        if !self.send_fcu {
            // maybe do something metrics-related here
            return None;
        }
        if let Some(provider) = &self.auth_provider {
            let provider = provider.clone();
            return Some(crate::spawn_with_session(async move {
                provider
                    .advance_chain(DEFAULT_BLOCK_TIME)
                    .await
                    .map_err(CallbackError::AuthProvider)
            }));
        }
        None
    }
}

impl OnBatchSent for NilCallback {
    fn on_batch_sent(&self) -> Option<JoinHandle<CallbackResult<()>>> {
        // do nothing
        None
    }
}

pub trait IntoCombinedCallback<B> {
    fn with_callback(self, custom: B) -> CombinedCallback<Self, B>
    where
        Self: Sized;
}

impl<T, B> IntoCombinedCallback<B> for T
where
    T: SpamCallback,
{
    fn with_callback(self, custom: B) -> CombinedCallback<Self, B> {
        CombinedCallback::new(self, custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::TxHash;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct Counter(Arc<AtomicUsize>);

    impl OnTxSent for Counter {
        fn on_tx_sent(
            &self,
            _: PendingTransactionConfig,
            _: &NamedTxRequest,
            _: RuntimeTxInfo,
            _: Option<HashMap<String, Arc<TxActorHandle>>>,
        ) -> Option<JoinHandle<CallbackResult<()>>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    impl OnBatchSent for Counter {
        fn on_batch_sent(&self) -> Option<JoinHandle<CallbackResult<()>>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            None
        }
    }

    #[tokio::test]
    async fn combined_callback_runs_both_sides() {
        let base = Counter::default();
        let custom = Counter::default();
        let base_count = base.0.clone();
        let custom_count = custom.0.clone();

        let combined = CombinedCallback::new(base, custom);
        let req = NamedTxRequest::new(Default::default(), None, None);
        combined.on_tx_sent(
            PendingTransactionConfig::new(TxHash::ZERO),
            &req,
            RuntimeTxInfo::default(),
            None,
        );
        combined.on_batch_sent();

        assert_eq!(base_count.load(Ordering::SeqCst), 2);
        assert_eq!(custom_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn callbacks_can_stack() {
        let counter1 = Counter::default();
        let counter2 = Counter::default();
        let counter3 = Counter::default();

        let combined = counter1
            .clone()
            .with_callback(counter2.clone())
            .with_callback(counter3.clone());
        /* NOTE:
            Clones are only necessary for this test.
            Real impls would likely not need to keep ownership of the callbacks after
            combining them, so they could be moved instead of cloned.
            Either way, ownership and cloning semantics are up to the caller
            since `CombinedCallback` is just a thin wrapper with no special handling.
        */

        let req = NamedTxRequest::new(Default::default(), None, None);
        combined.on_tx_sent(
            PendingTransactionConfig::new(TxHash::ZERO),
            &req,
            RuntimeTxInfo::default(),
            None,
        );
        combined.on_batch_sent();

        assert_eq!(counter1.0.load(Ordering::SeqCst), 2);
        assert_eq!(counter2.0.load(Ordering::SeqCst), 2);
        assert_eq!(counter3.0.load(Ordering::SeqCst), 2);
    }
}
