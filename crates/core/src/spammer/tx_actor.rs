use std::{collections::HashMap, sync::Arc, time::Duration};

use alloy::{
    network::{AnyReceiptEnvelope, ReceiptResponse},
    primitives::TxHash,
    providers::Provider,
    rpc::types::TransactionReceipt,
    serde::WithOtherFields,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};
use url::Url;

use crate::{
    db::{DbOps, RunTx},
    flashblocks::{FlashblockMark, FlashblocksClient},
    generator::types::AnyProvider,
    spammer::CallbackError,
    Result,
};

/// Maximum number of buffered flashblock marks for txs not yet in cache.
/// Legitimate marks are consumed within milliseconds when SentRunTx arrives;
/// entries that linger are foreign tx hashes that will never be claimed.
const MAX_PENDING_FLASHBLOCK_MARKS: usize = 1024;

/// External messages from API callers
pub enum TxActorMessage {
    InitCtx(ActorContext),
    GetCacheLen(oneshot::Sender<usize>),
    SentRunTx {
        tx_hash: TxHash,
        start_timestamp_ms: u128,
        end_timestamp_ms: Option<u128>,
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
    /// Replace the flush receiver with a new one (used when restarting the flush loop).
    ReplaceFlushReceiver(mpsc::Receiver<FlushRequest>),
    /// Clear the pending tx cache without writing to the DB.
    ClearCache {
        on_clear: oneshot::Sender<()>,
    },
}

impl std::fmt::Debug for TxActorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitCtx(ctx) => f.debug_tuple("InitCtx").field(ctx).finish(),
            Self::GetCacheLen(_) => f.write_str("GetCacheLen"),
            Self::SentRunTx { tx_hash, .. } => f
                .debug_struct("SentRunTx")
                .field("tx_hash", tx_hash)
                .finish(),
            Self::RemovedRunTx { tx_hash, .. } => f
                .debug_struct("RemovedRunTx")
                .field("tx_hash", tx_hash)
                .finish(),
            Self::DumpCache { run_id, .. } => {
                f.debug_struct("DumpCache").field("run_id", run_id).finish()
            }
            Self::Stop { .. } => f.write_str("Stop"),
            Self::ReplaceFlushReceiver(_) => f.write_str("ReplaceFlushReceiver"),
            Self::ClearCache { .. } => f.write_str("ClearCache"),
        }
    }
}

/// Internal messages from flush task to message handler
pub enum FlushRequest {
    /// Request a snapshot of the current cache
    GetSnapshot {
        reply: oneshot::Sender<(Vec<PendingRunTx>, Option<ActorContext>)>,
    },
    /// Remove confirmed txs and update target block
    RemoveConfirmed {
        confirmed_tx_hashes: Vec<TxHash>,
        new_target_block: u64,
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
    /// Dedicated flashblock marks receiver (large buffer to handle high TPS)
    flashblock_receiver: mpsc::Receiver<FlashblockMark>,
    /// dedicated stop signal receiver (separate from API messages to ensure it is always received)
    stop_receiver: mpsc::Receiver<()>,
    db: Arc<D>,
    cache: HashMap<TxHash, PendingRunTx>,
    ctx: Option<ActorContext>,
    status: ActorStatus,
    /// Flashblock marks that arrived before the tx was added to cache.
    /// Applied retroactively when SentRunTx is processed.
    /// Capped at [`MAX_PENDING_FLASHBLOCK_MARKS`] to bound memory from unrecognized tx hashes.
    pending_flashblock_marks: HashMap<TxHash, (u128, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingRunTx {
    pub tx_hash: TxHash,
    pub start_timestamp_ms: u128,
    pub end_timestamp_ms: Option<u128>,
    pub kind: Option<String>,
    pub error: Option<String>,
    pub flashblock_timestamp_ms: Option<u128>,
    pub flashblock_index: Option<u64>,
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
            end_timestamp_ms: None,
            kind: kind.map(|s| s.to_owned()),
            error: error.map(|s| s.to_owned()),
            flashblock_timestamp_ms: None,
            flashblock_index: None,
        }
    }

    /// Flashblock inclusion latency (time from send to first flashblock appearance).
    pub fn flashblock_latency_ms(&self) -> Option<u64> {
        self.flashblock_timestamp_ms.map(|fb_ts| {
            fb_ts
                .saturating_sub(self.start_timestamp_ms)
                .try_into()
                .unwrap()
        })
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
        flashblock_receiver: mpsc::Receiver<FlashblockMark>,
        stop_receiver: mpsc::Receiver<()>,
        db: Arc<D>,
    ) -> Self {
        Self {
            receiver,
            flush_receiver,
            flashblock_receiver,
            stop_receiver,
            db,
            cache: HashMap::new(),
            ctx: None,
            status: ActorStatus::default(),
            pending_flashblock_marks: HashMap::new(),
        }
    }

    /// Dumps all cached txs into the DB. Does not assign `block_number` or `gas_used`.
    /// If a tx has an `end_timestamp_ms` (from sync RPC), it is converted to seconds.
    fn dump_cache(&mut self, run_id: u64) -> Result<Vec<RunTx>> {
        let run_txs: Vec<_> = self
            .cache
            .values()
            .map(|pending_tx| RunTx {
                tx_hash: pending_tx.tx_hash,
                start_timestamp_ms: pending_tx.start_timestamp_ms.try_into().unwrap(),
                end_timestamp_ms: pending_tx.end_timestamp_ms.map(|ms| ms.try_into().unwrap()),
                block_number: None,
                gas_used: None,
                kind: pending_tx.kind.to_owned(),
                error: pending_tx.error.to_owned(),
                flashblock_latency_ms: pending_tx.flashblock_latency_ms(),
                flashblock_index: pending_tx.flashblock_index,
            })
            .collect();
        self.db
            .insert_run_txs(run_id, &run_txs)
            .map_err(|e| e.into())?;
        self.cache.clear();
        Ok(run_txs)
    }

    fn remove_cached_tx(&mut self, old_tx_hash: TxHash) -> Result<()> {
        self.cache
            .remove(&old_tx_hash)
            .ok_or(CallbackError::CacheRemoveTx(old_tx_hash))?;
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
                self.flashblock_receiver.close();
                self.flush_receiver.close();
                self.receiver.close();
                self.status = ActorStatus::ShuttingDown;
                on_stop
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("Stop: {:?}", e)))?;
            }
            TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp_ms,
                end_timestamp_ms,
                kind,
                error,
                on_receive,
            } => {
                // Check if a flashblock mark arrived before the tx was cached
                let (fb_timestamp, fb_index) =
                    if let Some((ts, idx)) = self.pending_flashblock_marks.remove(&tx_hash) {
                        debug!(
                            "applied buffered flashblock mark for tx {} (index={})",
                            tx_hash, idx
                        );
                        (Some(ts), Some(idx))
                    } else {
                        (None, None)
                    };
                let run_tx = PendingRunTx {
                    tx_hash,
                    start_timestamp_ms,
                    end_timestamp_ms,
                    kind,
                    error,
                    flashblock_timestamp_ms: fb_timestamp,
                    flashblock_index: fb_index,
                };
                self.cache.insert(tx_hash, run_tx);
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
            TxActorMessage::ReplaceFlushReceiver(new_receiver) => {
                self.flush_receiver = new_receiver;
            }
            TxActorMessage::ClearCache { on_clear } => {
                self.cache.clear();
                on_clear
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("ClearCache: {:?}", e)))?;
            }
        }
        Ok(())
    }

    /// Handle internal flush request
    fn handle_flush_request(&mut self, request: FlushRequest) {
        match request {
            FlushRequest::GetSnapshot { reply } => {
                // Send snapshot of cache and context
                let snapshot: Vec<PendingRunTx> = self.cache.values().cloned().collect();
                let _ = reply.send((snapshot, self.ctx.clone()));
            }
            FlushRequest::RemoveConfirmed {
                confirmed_tx_hashes,
                new_target_block,
            } => {
                // Remove confirmed txs (already recorded in DB by process_block_receipts)
                for hash in &confirmed_tx_hashes {
                    self.cache.remove(hash);
                }
                // Clean up any stale pending marks for confirmed txs
                for hash in &confirmed_tx_hashes {
                    self.pending_flashblock_marks.remove(hash);
                }
                // Evict buffered flashblock marks older than 20s.
                // Legitimate marks are consumed within milliseconds; anything
                // older is a foreign tx hash that will never be claimed.
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis();
                self.pending_flashblock_marks
                    .retain(|_, (ts, _)| now_ms.saturating_sub(*ts) < 20_000);
                // Update target block
                if let Some(ref mut ctx) = self.ctx {
                    if new_target_block > ctx.target_block {
                        ctx.target_block = new_target_block;
                    }
                }
            }
        }
    }

    /// Handle a flashblock mark from the dedicated flashblock channel
    fn handle_flashblock_mark(&mut self, mark: FlashblockMark) {
        // First match wins — only set if not already set
        if let Some(pending_tx) = self.cache.get_mut(&mark.tx_hash) {
            if pending_tx.flashblock_timestamp_ms.is_none() {
                pending_tx.flashblock_timestamp_ms = Some(mark.timestamp_ms);
                pending_tx.flashblock_index = Some(mark.index);
                debug!(
                    "marked flashblock for tx {} (index={})",
                    mark.tx_hash, mark.index
                );
            }
        } else if self.pending_flashblock_marks.len() >= MAX_PENDING_FLASHBLOCK_MARKS {
            warn!(
                "pending_flashblock_marks at capacity ({}); dropping mark for tx {}",
                MAX_PENDING_FLASHBLOCK_MARKS, mark.tx_hash
            );
        } else {
            // Tx not in cache yet (SentRunTx hasn't been processed).
            // Buffer the mark so it can be applied when the tx arrives.
            self.pending_flashblock_marks
                .entry(mark.tx_hash)
                .or_insert((mark.timestamp_ms, mark.index));
        }
    }

    /// Main loop: handles both external messages and internal flush requests
    pub async fn run(&mut self) -> Result<()> {
        loop {
            if self.status == ActorStatus::ShuttingDown {
                break;
            }

            tokio::select! {
                _ = self.stop_receiver.recv() => {
                    // Received stop signal, begin shutdown process
                    let (sender, receiver) = oneshot::channel();
                    self.handle_message(TxActorMessage::Stop { on_stop: sender })?;
                    // Wait for confirmation that shutdown message was processed
                    let _ = receiver.await;
                    debug!("TxActor has shut down.");
                }
                msg = self.receiver.recv() => {
                    tracing::trace!("TxActor received a message");
                    if let Some(msg) = msg {
                        tracing::trace!("message is Some, handling it");
                        self.handle_message(msg)?;
                    }
                }

                req = self.flush_receiver.recv() => {
                    tracing::trace!("TxActor received a flush message");
                    if let Some(req) = req {
                        tracing::trace!("flush_request is Some, handling it");
                        self.handle_flush_request(req);
                    }
                }

                mark = self.flashblock_receiver.recv() => {
                    tracing::trace!("TxActor received a flashblock message");
                    if let Some(mark) = mark {
                        tracing::trace!("flashblock mark is Some, handling it");
                        self.handle_flashblock_mark(mark);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Standalone flush task - communicates with message handler via channels
async fn flush_loop<D: DbOps + Send + Sync + 'static>(
    flush_sender: mpsc::Sender<FlushRequest>,
    db: Arc<D>,
    rpc: Arc<AnyProvider>,
    cancel_token: CancellationToken,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    /// Number of consecutive blocks with no change in pending count before giving up.
    const STALE_BLOCK_LIMIT: u64 = 6;
    let mut stale_blocks: u64 = 0;
    let mut last_pending_count: usize = usize::MAX;

    // Phase 1: Wait for context to be initialized.
    // Avoids burning CPU with snapshot round-trips when no spam run is active.
    let mut target_block = loop {
        interval.tick().await;

        let (reply_tx, reply_rx) = oneshot::channel();
        if flush_sender
            .send(FlushRequest::GetSnapshot { reply: reply_tx })
            .await
            .is_err()
        {
            return;
        }

        match reply_rx.await {
            Ok((_, Some(ctx))) => break ctx.target_block,
            Ok((_, None)) => {
                trace!("TxActor context not initialized.");
            }
            Err(_) => return,
        }
    };

    // Phase 2: Process blocks as they arrive.
    loop {
        interval.tick().await;

        // Check for new blocks BEFORE requesting a snapshot to avoid unnecessary channel traffic.
        let new_block = match rpc.get_block_number().await {
            Ok(n) => n,
            Err(e) => {
                warn!("Failed to get block number: {:?}", e);
                continue;
            }
        };

        if target_block >= new_block {
            // No new blocks; if cancel is set, check whether the cache is already empty.
            if cancel_token.is_cancelled() {
                let (reply_tx, reply_rx) = oneshot::channel();
                if flush_sender
                    .send(FlushRequest::GetSnapshot { reply: reply_tx })
                    .await
                    .is_err()
                {
                    break;
                }
                if let Ok((snapshot, _)) = reply_rx.await {
                    if snapshot.is_empty() {
                        info!("all receipts processed, shutting down receipt collection.");
                        break;
                    }
                }
            }
            continue;
        }

        // New blocks available — request snapshot now.
        let (reply_tx, reply_rx) = oneshot::channel();
        if flush_sender
            .send(FlushRequest::GetSnapshot { reply: reply_tx })
            .await
            .is_err()
        {
            break;
        }

        let (mut cache_snapshot, ctx) = match reply_rx.await {
            Ok((snapshot, Some(ctx))) => {
                target_block = ctx.target_block;
                (snapshot, ctx)
            }
            Ok((_, None)) => continue,
            Err(_) => break,
        };

        // Re-check after context refresh.
        if target_block >= new_block {
            continue;
        }

        // If cancel_token is set and cache is empty, we're done.
        if cancel_token.is_cancelled() && cache_snapshot.is_empty() {
            info!("all receipts processed, shutting down receipt collection.");
            break;
        }

        let blocks_start = target_block;

        // Process blocks one at a time, refreshing the snapshot after each.
        for bn in target_block..new_block {
            match process_block_receipts(&cache_snapshot, &db, &rpc, ctx.run_id, bn).await {
                Ok(confirmed) => {
                    let _ = flush_sender
                        .send(FlushRequest::RemoveConfirmed {
                            confirmed_tx_hashes: confirmed,
                            new_target_block: bn + 1,
                        })
                        .await;

                    // Refresh snapshot so next block sees updated cache
                    let (reply_tx, reply_rx) = oneshot::channel();
                    if flush_sender
                        .send(FlushRequest::GetSnapshot { reply: reply_tx })
                        .await
                        .is_err()
                    {
                        return;
                    }
                    match reply_rx.await {
                        Ok((new_snapshot, Some(ctx))) => {
                            cache_snapshot = new_snapshot;
                            target_block = ctx.target_block;
                        }
                        Ok((_, None)) => break,
                        Err(_) => break,
                    }

                    if cache_snapshot.is_empty() {
                        if cancel_token.is_cancelled() {
                            info!("pending tx cache is empty, shutting down receipt collection.");
                            return;
                        }
                        break;
                    }
                }
                Err(e) => warn!("flush_cache error for block {}: {:?}", bn, e),
            }
        }

        // Ensure we advance past all processed blocks.
        target_block = target_block.max(new_block);

        // Track stale blocks: if sending is done and pending count hasn't changed, increment.
        if cancel_token.is_cancelled() && !cache_snapshot.is_empty() {
            let current_count = cache_snapshot.len();
            if current_count == last_pending_count {
                stale_blocks += new_block.saturating_sub(blocks_start).max(1);
            } else {
                stale_blocks = 0;
                last_pending_count = current_count;
            }
            if stale_blocks >= STALE_BLOCK_LIMIT {
                warn!(
                    "pending receipt count unchanged ({}) for {} blocks, shutting down receipt collection.",
                    current_count, stale_blocks
                );
                break;
            }
        }
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
    if cache_snapshot.is_empty() {
        return Ok(Vec::new());
    } else {
        info!("unconfirmed txs: {}", cache_snapshot.len());
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
    let receipts = match rpc.get_block_receipts(target_block_num.into()).await {
        Ok(Some(r)) => r,
        Ok(None) | Err(_) => {
            // Fallback: fetch receipts individually in parallel
            let receipt_futures: Vec<_> = target_block
                .transactions
                .hashes()
                .map(|tx_hash| rpc.get_transaction_receipt(tx_hash))
                .collect();
            let results = futures::future::join_all(receipt_futures).await;
            results
                .into_iter()
                .filter_map(|r| r.ok().flatten())
                .collect()
        }
    };
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
                start_timestamp_ms: pending_tx.start_timestamp_ms.try_into().unwrap(),
                end_timestamp_ms: Some(
                    pending_tx
                        .end_timestamp_ms
                        .map(|ms| ms.try_into().unwrap())
                        .unwrap_or(target_block.header.timestamp * 1000),
                ),
                block_number: Some(target_block.header.number),
                gas_used: Some(receipt.gas_used),
                kind: pending_tx.kind.clone(),
                error: get_tx_error(receipt, pending_tx),
                flashblock_latency_ms: pending_tx.flashblock_latency_ms(),
                flashblock_index: pending_tx.flashblock_index,
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
    stop_sender: mpsc::Sender<()>,
    flush_complete: std::sync::Mutex<CancellationToken>,
    // we need to keep the sender side of the flashblock channel here to prevent it from being dropped.
    fb_sender: mpsc::Sender<FlashblockMark>,
}

#[derive(Debug)]
pub struct CacheTx {
    pub tx_hash: TxHash,
    pub start_timestamp_ms: u128,
    pub end_timestamp_ms: Option<u128>,
    pub kind: Option<String>,
    pub error: Option<String>,
}

impl TxActorHandle {
    pub async fn new<D: DbOps + Send + Sync + 'static>(
        bufsize: usize,
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
        flashblocks_ws_url: Option<Url>,
        cancel_token: CancellationToken,
    ) -> Result<Self> {
        // Pre-flight check: validate flashblocks WS endpoint before spawning tasks
        if let Some(ref ws_url) = flashblocks_ws_url {
            FlashblocksClient::preflight(ws_url).await?;
        }

        // dedicated stop channel to signal shutdown (separate from API messages to ensure it is always received)
        let (stop_sender, stop_receiver) = mpsc::channel(1);
        // generic channel for API messages to the actor (cache updates, dump requests, etc.)
        let (sender, receiver) = mpsc::channel(bufsize);
        // Channel for flush task to communicate with message handler
        let (flush_sender, flush_receiver) = mpsc::channel(64);
        // Dedicated channel for flashblock marks (large buffer to handle high TPS)
        let (fb_sender, fb_receiver) = mpsc::channel(10_000);

        let mut actor = TxActor::new(
            receiver,
            flush_receiver,
            fb_receiver,
            stop_receiver,
            db.clone(),
        );

        // Spawn the message handler task (owns the cache)
        crate::spawn_with_session(async move {
            if let Err(e) = actor.run().await {
                error!("TxActor message handler terminated with error: {:?}", e);
            }
        });

        // Spawn the independent flush task (communicates via channels)
        let flush_cancel = cancel_token.clone();
        let flush_complete = CancellationToken::new();
        let flush_done = flush_complete.clone();
        crate::spawn_with_session(async move {
            flush_loop(flush_sender, db, rpc, flush_cancel).await;
            flush_done.cancel();
        });

        // Spawn the flashblocks listener task if URL is provided
        if let Some(ws_url) = flashblocks_ws_url {
            let fb_sender = fb_sender.clone();
            crate::spawn_with_session(async move {
                if let Err(e) = FlashblocksClient::listen(&ws_url, fb_sender, cancel_token).await {
                    error!("{}", e);
                }
            });
        }

        Ok(Self {
            sender,
            flush_complete: std::sync::Mutex::new(flush_complete),
            fb_sender,
            stop_sender,
        })
    }

    /// Waits until the flush loop has finished processing all receipts.
    pub async fn await_flush(&self) {
        let token = self.flush_complete.lock().unwrap().clone();
        token.cancelled().await;
    }

    /// Restart the flush loop for a new spam run.
    ///
    /// Creates a new flush channel pair, sends the new receiver to the actor,
    /// and spawns a fresh flush loop with the given cancel token.
    pub async fn restart_flush<D: DbOps + Send + Sync + 'static>(
        &self,
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        let (flush_sender, flush_receiver) = mpsc::channel(64);

        // Send new flush_receiver to actor
        self.sender
            .send(TxActorMessage::ReplaceFlushReceiver(flush_receiver))
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;

        // Replace flush_complete token
        let flush_complete = CancellationToken::new();
        let flush_done = flush_complete.clone();
        *self.flush_complete.lock().unwrap() = flush_complete;

        // Spawn new flush loop
        crate::spawn_with_session(async move {
            flush_loop(flush_sender, db, rpc, cancel_token).await;
            flush_done.cancel();
        });

        Ok(())
    }

    /// Adds a new tx to the cache.
    pub async fn cache_run_tx(&self, params: CacheTx) -> Result<()> {
        let CacheTx {
            tx_hash,
            start_timestamp_ms,
            end_timestamp_ms,
            kind,
            error,
        } = params;
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp_ms,
                end_timestamp_ms,
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

    pub async fn done_flushing(&self) -> Result<usize> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::GetCacheLen(sender))
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        let cache_len = receiver.await.map_err(CallbackError::from)?;
        Ok(cache_len)
    }

    pub fn is_fb_stream_closed(&self) -> bool {
        self.fb_sender.is_closed()
    }

    pub async fn reopen_fb_stream(
        &mut self,
        ws_url: Url,
        cancel_token: CancellationToken,
    ) -> Result<tokio::sync::mpsc::Receiver<FlashblockMark>> {
        let (fb_sender, fb_receiver) = mpsc::channel(10_000);
        self.fb_sender.clone_from(&fb_sender);
        crate::spawn_with_session(async move {
            if let Err(e) = FlashblocksClient::listen(&ws_url, fb_sender, cancel_token).await {
                error!("{}", e);
            }
        });
        Ok(fb_receiver)
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

    /// Clears the pending tx cache without writing to the DB.
    pub async fn clear_cache(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::ClearCache { on_clear: sender })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    /// Stops the actor, terminating all pending tasks.
    pub async fn stop(&self) -> Result<()> {
        self.stop_sender
            .send(())
            .await
            .map_err(|_| CallbackError::Stop.into())
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
