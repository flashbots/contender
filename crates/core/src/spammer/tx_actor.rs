use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use alloy::{
    network::{AnyReceiptEnvelope, ReceiptResponse},
    primitives::TxHash,
    providers::Provider,
    rpc::types::TransactionReceipt,
    serde::WithOtherFields,
};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::{
    db::{DbOps, RunTx},
    generator::types::AnyProvider,
    spammer::CallbackError,
    Result,
};

/// External messages from API callers
#[derive(Debug)]
pub enum TxActorMessage {
    InitCtx(ActorContext),
    GetCacheLen(oneshot::Sender<usize>),
    SentRunTx {
        tx_hash: TxHash,
        start_timestamp_ms: u128,
        kind: Option<String>,
        error: Option<String>,
        on_receive: oneshot::Sender<()>,
    },
    RemovedRunTx {
        tx_hash: TxHash,
        on_remove: oneshot::Sender<()>,
    },
    DumpCache {
        run_id: u64,
        on_dump_cache: oneshot::Sender<Vec<RunTx>>,
    },
    Stop {
        on_stop: oneshot::Sender<()>,
    },
}

/// Internal messages from flush task to message handler
enum FlushRequest {
    /// Request a snapshot of the current cache
    GetSnapshot {
        reply: oneshot::Sender<(Vec<PendingRunTx>, Option<ActorContext>)>,
    },
    /// Remove confirmed txs and update target block
    RemoveConfirmed {
        confirmed_tx_hashes: Vec<TxHash>,
        new_target_block: u64,
        pending_tx_timeout: Duration,
    },
}

struct TxActor<D>
where
    D: DbOps,
{
    /// External message receiver
    receiver: mpsc::Receiver<TxActorMessage>,
    /// Internal flush request receiver
    flush_receiver: mpsc::Receiver<FlushRequest>,
    db: Arc<D>,
    cache: Vec<PendingRunTx>,
    ctx: Option<ActorContext>,
    status: ActorStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingRunTx {
    pub tx_hash: TxHash,
    pub start_timestamp_ms: u128,
    pub kind: Option<String>,
    pub error: Option<String>,
}

impl PendingRunTx {
    pub fn new(
        tx_hash: TxHash,
        start_timestamp_ms: u128,
        kind: Option<&str>,
        error: Option<&str>,
    ) -> Self {
        Self {
            tx_hash,
            start_timestamp_ms,
            kind: kind.map(|s| s.to_owned()),
            error: error.map(|s| s.to_owned()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActorContext {
    pub run_id: u64,
    pub target_block: u64,
    pub pending_tx_timeout: Duration,
}

impl ActorContext {
    pub fn new(target_block: u64, run_id: u64) -> Self {
        Self {
            run_id,
            target_block,
            pending_tx_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_pending_tx_timeout(mut self, timeout: Duration) -> Self {
        self.pending_tx_timeout = timeout;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ActorStatus {
    ShuttingDown,
    #[default]
    Running,
}

impl<D> TxActor<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(
        receiver: mpsc::Receiver<TxActorMessage>,
        flush_receiver: mpsc::Receiver<FlushRequest>,
        db: Arc<D>,
    ) -> Self {
        Self {
            receiver,
            flush_receiver,
            db,
            cache: Vec::new(),
            ctx: None,
            status: ActorStatus::default(),
        }
    }

    /// Dumps all cached txs into the DB. Does not assign `end_timestamp`, `block_number`, or `gas_used`.
    fn dump_cache(&mut self, run_id: u64) -> Result<Vec<RunTx>> {
        let run_txs: Vec<_> = self
            .cache
            .iter()
            .map(|pending_tx| RunTx {
                tx_hash: pending_tx.tx_hash,
                start_timestamp_secs: (pending_tx.start_timestamp_ms / 1000) as u64,
                end_timestamp_secs: None,
                block_number: None,
                gas_used: None,
                kind: pending_tx.kind.to_owned(),
                error: pending_tx.error.to_owned(),
            })
            .collect();
        self.db
            .insert_run_txs(run_id, &run_txs)
            .map_err(|e| e.into())?;
        self.cache.clear();
        Ok(run_txs)
    }

    fn remove_cached_tx(&mut self, old_tx_hash: TxHash) -> Result<()> {
        let old_tx = self
            .cache
            .iter()
            .position(|tx| tx.tx_hash == old_tx_hash)
            .ok_or(CallbackError::CacheRemoveTx(old_tx_hash))?;
        self.cache.remove(old_tx);
        Ok(())
    }

    /// Handle external API message
    fn handle_message(&mut self, message: TxActorMessage) -> Result<()> {
        match message {
            TxActorMessage::GetCacheLen(on_len) => {
                on_len
                    .send(self.cache.len())
                    .map_err(|e| CallbackError::OneshotSend(format!("GetCacheLen: {:?}", e)))?;
            }
            TxActorMessage::InitCtx(ctx) => {
                self.ctx = Some(ctx);
            }
            TxActorMessage::Stop { on_stop } => {
                self.status = ActorStatus::ShuttingDown;
                on_stop.send(()).map_err(|_| CallbackError::Stop)?;
            }
            TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp_ms,
                kind,
                error,
                on_receive,
            } => {
                let run_tx = PendingRunTx {
                    tx_hash,
                    start_timestamp_ms,
                    kind,
                    error,
                };
                self.cache.push(run_tx);
                on_receive
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("SentRunTx: {:?}", e)))?;
            }
            TxActorMessage::RemovedRunTx { tx_hash, on_remove } => {
                self.remove_cached_tx(tx_hash)?;
                on_remove
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("RemovedRunTx: {:?}", e)))?;
            }
            TxActorMessage::DumpCache {
                on_dump_cache,
                run_id,
            } => {
                let res = self.dump_cache(run_id)?;
                on_dump_cache.send(res).map_err(CallbackError::DumpCache)?;
            }
        }
        Ok(())
    }

    /// Handle internal flush request
    fn handle_flush_request(&mut self, request: FlushRequest) {
        match request {
            FlushRequest::GetSnapshot { reply } => {
                // Send snapshot of cache and context
                let _ = reply.send((self.cache.clone(), self.ctx.clone()));
            }
            FlushRequest::RemoveConfirmed {
                confirmed_tx_hashes,
                new_target_block,
                pending_tx_timeout,
            } => {
                // Remove confirmed txs and timed-out txs
                self.cache.retain(|tx| {
                    let is_confirmed = confirmed_tx_hashes.contains(&tx.tx_hash);
                    let is_timed_out =
                        is_tx_timed_out_ms(tx.start_timestamp_ms, pending_tx_timeout);
                    if is_timed_out && !is_confirmed {
                        debug!(
                            "tx timed out after {:?}: {:?}",
                            pending_tx_timeout, tx.tx_hash
                        );
                    }
                    !is_confirmed && !is_timed_out
                });
                // Update target block
                if let Some(ref mut ctx) = self.ctx {
                    if new_target_block > ctx.target_block {
                        ctx.target_block = new_target_block;
                    }
                }
            }
        }
    }

    /// Main loop: handles both external messages and internal flush requests
    pub async fn run(&mut self) -> Result<()> {
        loop {
            if self.status == ActorStatus::ShuttingDown {
                break;
            }

            tokio::select! {
                // External messages have priority (biased)
                biased;

                msg = self.receiver.recv() => {
                    if let Some(msg) = msg {
                        self.handle_message(msg)?;
                    }
                }

                req = self.flush_receiver.recv() => {
                    if let Some(req) = req {
                        self.handle_flush_request(req);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Check if tx is timed out based on start timestamp
fn is_tx_timed_out_ms(start_timestamp_ms: u128, timeout: Duration) -> bool {
    let duration_since_epoch = Duration::from_millis(start_timestamp_ms as u64);
    let timestamp = std::time::UNIX_EPOCH + duration_since_epoch;
    match SystemTime::now().duration_since(timestamp) {
        Ok(elapsed) => elapsed > timeout,
        Err(_) => true,
    }
}

/// Standalone flush task - communicates with message handler via channels
async fn flush_loop<D: DbOps + Send + Sync + 'static>(
    flush_sender: mpsc::Sender<FlushRequest>,
    db: Arc<D>,
    rpc: Arc<AnyProvider>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        // Request snapshot from message handler
        let (reply_tx, reply_rx) = oneshot::channel();
        if flush_sender
            .send(FlushRequest::GetSnapshot { reply: reply_tx })
            .await
            .is_err()
        {
            // Message handler shut down
            break;
        }

        let (cache_snapshot, ctx) = match reply_rx.await {
            Ok(data) => data,
            Err(_) => continue,
        };

        let Some(ctx) = ctx else {
            debug!("TxActor context not initialized.");
            continue;
        };

        // Get current block number
        let new_block = match rpc.get_block_number().await {
            Ok(n) => n,
            Err(e) => {
                warn!("Failed to get block number: {:?}", e);
                continue;
            }
        };

        if ctx.target_block >= new_block {
            continue;
        }

        // Process all pending blocks
        let mut all_confirmed_hashes = Vec::new();
        for bn in ctx.target_block..new_block {
            match process_block_receipts(&cache_snapshot, &db, &rpc, ctx.run_id, bn).await {
                Ok(confirmed) => all_confirmed_hashes.extend(confirmed),
                Err(e) => warn!("flush_cache error for block {}: {:?}", bn, e),
            }
        }

        // Send removal request back to message handler
        let _ = flush_sender
            .send(FlushRequest::RemoveConfirmed {
                confirmed_tx_hashes: all_confirmed_hashes,
                new_target_block: new_block,
                pending_tx_timeout: ctx.pending_tx_timeout,
            })
            .await;
    }
}

/// Process receipts for a single block, return confirmed tx hashes
async fn process_block_receipts<D: DbOps + Send + Sync + 'static>(
    cache_snapshot: &[PendingRunTx],
    db: &Arc<D>,
    rpc: &Arc<AnyProvider>,
    run_id: u64,
    target_block_num: u64,
) -> Result<Vec<TxHash>> {
    info!("unconfirmed txs: {}", cache_snapshot.len());

    if cache_snapshot.is_empty() {
        return Ok(Vec::new());
    }

    // Wait for block to appear
    let target_block = loop {
        match rpc.get_block_by_number(target_block_num.into()).await {
            Ok(Some(block)) => break block,
            Ok(None) => {
                info!("waiting for block {target_block_num}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                warn!("Error fetching block {}: {:?}", target_block_num, e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };

    // Get receipts
    let receipts = rpc
        .get_block_receipts(target_block_num.into())
        .await?
        .unwrap_or_default();
    info!(
        "found {} receipts for block {}",
        receipts.len(),
        target_block_num
    );

    // Find confirmed txs by matching with receipts
    let confirmed: Vec<_> = cache_snapshot
        .iter()
        .filter_map(|pending_tx| {
            receipts
                .iter()
                .find(|r| r.transaction_hash == pending_tx.tx_hash)
                .map(|receipt| (pending_tx, receipt))
        })
        .collect();

    // Build RunTx records and insert to DB
    let run_txs: Vec<_> = confirmed
        .iter()
        .map(|(pending_tx, receipt)| {
            if !receipt.status() {
                warn!("tx failed: {:?}", pending_tx.tx_hash);
            } else {
                debug!(
                    "tx landed. hash: {}, gas_used: {}, block_num: {}",
                    pending_tx.tx_hash,
                    receipt.gas_used,
                    receipt
                        .block_number
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "N/A".to_owned())
                );
            }
            RunTx {
                tx_hash: pending_tx.tx_hash,
                start_timestamp_secs: (pending_tx.start_timestamp_ms / 1000) as u64,
                end_timestamp_secs: Some(target_block.header.timestamp),
                block_number: Some(target_block.header.number),
                gas_used: Some(receipt.gas_used),
                kind: pending_tx.kind.clone(),
                error: get_tx_error(receipt, pending_tx),
            }
        })
        .collect();

    if !run_txs.is_empty() {
        db.insert_run_txs(run_id, &run_txs).map_err(|e| e.into())?;
    }

    Ok(confirmed.iter().map(|(tx, _)| tx.tx_hash).collect())
}

/// Return tx error based on receipt status.
fn get_tx_error(
    receipt: &WithOtherFields<TransactionReceipt<AnyReceiptEnvelope<alloy::rpc::types::Log>>>,
    pending_tx: &PendingRunTx,
) -> Option<String> {
    if receipt.status() {
        pending_tx.error.clone()
    } else {
        Some("execution reverted".to_string())
    }
}

#[derive(Debug)]
pub struct TxActorHandle {
    sender: mpsc::Sender<TxActorMessage>,
}

#[derive(Debug)]
pub struct CacheTx {
    pub tx_hash: TxHash,
    pub start_timestamp_ms: u128,
    pub kind: Option<String>,
    pub error: Option<String>,
}

impl TxActorHandle {
    pub fn new<D: DbOps + Send + Sync + 'static>(
        bufsize: usize,
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(bufsize);
        // Channel for flush task to communicate with message handler
        // Small buffer since flush requests are infrequent
        let (flush_sender, flush_receiver) = mpsc::channel(16);

        let mut actor = TxActor::new(receiver, flush_receiver, db.clone());

        // Spawn the message handler task (owns the cache)
        tokio::task::spawn(async move {
            if let Err(e) = actor.run().await {
                error!("TxActor message handler terminated with error: {:?}", e);
            }
        });

        // Spawn the independent flush task (communicates via channels)
        tokio::task::spawn(async move {
            flush_loop(flush_sender, db, rpc).await;
        });

        Self { sender }
    }

    /// Adds a new tx to the cache.
    pub async fn cache_run_tx(&self, params: CacheTx) -> Result<()> {
        let CacheTx {
            tx_hash,
            start_timestamp_ms,
            kind,
            error,
        } = params;
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp_ms,
                kind,
                on_receive: sender,
                error,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    /// Dumps remaining txs in cache to the DB and returns them. Does not assign `end_timestamp`, `block_number`, or `gas_used`.
    pub async fn dump_cache(&self, run_id: u64) -> Result<Vec<RunTx>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::DumpCache {
                on_dump_cache: sender,
                run_id,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    pub async fn done_flushing(&self) -> Result<bool> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::GetCacheLen(sender))
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        let cache_len = receiver.await.map_err(CallbackError::from)?;
        Ok(cache_len == 0)
    }

    /// Removes an existing tx in the cache.
    pub async fn remove_cached_tx(&self, tx_hash: TxHash) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::RemovedRunTx {
                tx_hash,
                on_remove: sender,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;

        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    /// Stops the actor, terminating all pending tasks.
    pub async fn stop(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::Stop { on_stop: sender })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    pub async fn init_ctx(&self, ctx: ActorContext) -> Result<()> {
        self.sender
            .send(TxActorMessage::InitCtx(ctx))
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(())
    }
}
