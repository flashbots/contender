use std::{sync::Arc, time::Duration};

use alloy::{
    hex::FromHex,
    network::{AnyReceiptEnvelope, ReceiptResponse},
    primitives::TxHash,
    providers::Provider,
    rpc::types::TransactionReceipt,
    serde::WithOtherFields,
};
use futures::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};
use url::Url;

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
    },
    /// Mark a tx as seen in a flashblock (first match wins)
    MarkFlashblock {
        tx_hash: TxHash,
        timestamp_ms: u128,
        index: u64,
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
            kind: kind.map(|s| s.to_owned()),
            error: error.map(|s| s.to_owned()),
            flashblock_timestamp_ms: None,
            flashblock_index: None,
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
            .map(|pending_tx| {
                let flashblock_latency_ms = pending_tx.flashblock_timestamp_ms.map(|fb_ts| {
                    fb_ts.saturating_sub(pending_tx.start_timestamp_ms) as u64
                });
                RunTx {
                    tx_hash: pending_tx.tx_hash,
                    start_timestamp_ms: pending_tx.start_timestamp_ms as u64,
                    end_timestamp_ms: None,
                    block_number: None,
                    gas_used: None,
                    kind: pending_tx.kind.to_owned(),
                    error: pending_tx.error.to_owned(),
                    flashblock_latency_ms,
                    flashblock_index: pending_tx.flashblock_index,
                }
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
                    flashblock_timestamp_ms: None,
                    flashblock_index: None,
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
            } => {
                // Remove confirmed txs (already recorded in DB by process_block_receipts)
                self.cache
                    .retain(|tx| !confirmed_tx_hashes.contains(&tx.tx_hash));
                // Update target block
                if let Some(ref mut ctx) = self.ctx {
                    if new_target_block > ctx.target_block {
                        ctx.target_block = new_target_block;
                    }
                }
            }
            FlushRequest::MarkFlashblock {
                tx_hash,
                timestamp_ms,
                index,
            } => {
                // First match wins — only set if not already set
                if let Some(pending_tx) = self
                    .cache
                    .iter_mut()
                    .find(|tx| tx.tx_hash == tx_hash)
                {
                    if pending_tx.flashblock_timestamp_ms.is_none() {
                        pending_tx.flashblock_timestamp_ms = Some(timestamp_ms);
                        pending_tx.flashblock_index = Some(index);
                        debug!("marked flashblock for tx {} (index={})", tx_hash, index);
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

        // Process blocks one at a time, refreshing the snapshot after each
        let mut cache_snapshot = cache_snapshot;
        let run_id = ctx.run_id;

        for bn in ctx.target_block..new_block {
            match process_block_receipts(&cache_snapshot, &db, &rpc, run_id, bn).await {
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
                        Ok((new_snapshot, Some(_))) => {
                            cache_snapshot = new_snapshot;
                        }
                        Ok((_, None)) => break,
                        Err(_) => break,
                    }

                    if cache_snapshot.is_empty() {
                        break;
                    }
                }
                Err(e) => warn!("flush_cache error for block {}: {:?}", bn, e),
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
            let flashblock_latency_ms = pending_tx.flashblock_timestamp_ms.map(|fb_ts| {
                fb_ts.saturating_sub(pending_tx.start_timestamp_ms) as u64
            });
            RunTx {
                tx_hash: pending_tx.tx_hash,
                start_timestamp_ms: pending_tx.start_timestamp_ms as u64,
                end_timestamp_ms: Some(target_block.header.timestamp * 1000),
                block_number: Some(target_block.header.number),
                gas_used: Some(receipt.gas_used),
                kind: pending_tx.kind.clone(),
                error: get_tx_error(receipt, pending_tx),
                flashblock_latency_ms,
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

/// Extract UTF-8 text from a Text or Binary WebSocket message.
/// Flashblocks endpoints may send JSON as either frame type.
fn ws_message_to_text(msg: Message) -> Option<String> {
    match msg {
        Message::Text(t) => Some(t.to_string()),
        Message::Binary(b) => String::from_utf8(b.to_vec()).ok(),
        _ => None,
    }
}

/// Pre-flight check: connect to flashblocks WS endpoint and validate it serves flashblocks.
/// The endpoint auto-streams flashblock diffs on connect (no subscription needed).
/// We verify by waiting for a valid message with `metadata.receipts`.
async fn flashblocks_preflight(ws_url: &Url) -> Result<()> {
    info!("Validating flashblocks WS endpoint: {}", ws_url);

    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(ws_url.as_str())
            .await
            .map_err(|e| {
                crate::error::Error::Runtime(crate::error::RuntimeErrorKind::InvalidParams(
                    crate::error::RuntimeParamErrorKind::InvalidArgs(format!(
                        "Failed to connect to flashblocks WS endpoint {}: {}",
                        ws_url, e
                    )),
                ))
            })?;

    // Wait for a valid flashblock message (with timeout).
    // The endpoint auto-streams — no subscription handshake required.
    // Loop to skip non-data frames (e.g. Ping) until we get a Text/Binary message.
    let preflight_result = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg_result) = ws_stream.next().await {
            match msg_result {
                Ok(msg) => {
                    if let Some(text) = ws_message_to_text(msg) {
                        return Ok(text);
                    }
                    // Non-data frame (Ping, Pong, etc.) — skip and wait for next
                    continue;
                }
                Err(e) => {
                    return Err(format!("Flashblocks WS connection error during preflight: {e}"));
                }
            }
        }
        Err("Flashblocks WS connection closed during preflight".to_string())
    })
    .await
    .map_err(|_| {
        crate::error::Error::Runtime(crate::error::RuntimeErrorKind::InvalidParams(
            crate::error::RuntimeParamErrorKind::InvalidArgs(
                "Flashblocks WS endpoint did not send any data within 10 seconds".to_string(),
            ),
        ))
    })?
    .map_err(|msg| {
        crate::error::Error::Runtime(crate::error::RuntimeErrorKind::InvalidParams(
            crate::error::RuntimeParamErrorKind::InvalidArgs(msg),
        ))
    })?;

    let parsed: serde_json::Value =
        serde_json::from_str(&preflight_result).unwrap_or(serde_json::Value::Null);
    if parsed.get("metadata").and_then(|m| m.get("receipts")).is_some() {
        info!("Flashblocks WS endpoint validated successfully");
    } else {
        return Err(crate::error::Error::Runtime(
            crate::error::RuntimeErrorKind::InvalidParams(
                crate::error::RuntimeParamErrorKind::InvalidArgs(format!(
                    "Flashblocks WS endpoint sent unexpected message format (missing metadata.receipts): {}",
                    &preflight_result[..preflight_result.len().min(200)]
                )),
            ),
        ));
    }

    // Close the preflight connection
    let _ = ws_stream.close(None).await;

    Ok(())
}

/// Listens for flashblock diffs over WebSocket and marks matching pending txs.
/// The endpoint auto-streams flashblock diffs on connect — no subscription needed.
/// Each message is a JSON object with `metadata.receipts` containing tx hashes as keys.
async fn flashblocks_listener(flush_sender: mpsc::Sender<FlushRequest>, ws_url: Url) {
    loop {
        info!("Connecting to flashblocks WS: {}", ws_url);

        let ws_stream = match tokio_tungstenite::connect_async(ws_url.as_str()).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                warn!("Failed to connect to flashblocks WS: {:?}. Retrying in 2s...", e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        let (_write, mut read) = ws_stream.split();

        // Process incoming messages — endpoint auto-streams, no subscription needed
        while let Some(msg_result) = read.next().await {
            let msg = match msg_result {
                Ok(msg) => msg,
                Err(e) => {
                    warn!("Flashblocks WS error: {:?}. Reconnecting...", e);
                    break;
                }
            };

            if matches!(msg, Message::Close(_)) {
                info!("Flashblocks WS closed. Reconnecting...");
                break;
            }

            let text = match ws_message_to_text(msg) {
                Some(t) => t,
                None => continue,
            };

            let timestamp_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis();

            // Parse the flashblock diff message
            let parsed: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Flashblock diff format:
            // {"payload_id":"0x...","index":0,"diff":{"transactions":[...]},"metadata":{"receipts":{"0xTxHash1":{...},...}}}
            // Extract the flashblock index
            let index = parsed
                .get("index")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            // Extract tx hashes from metadata.receipts keys
            let tx_hashes: Vec<TxHash> = parsed
                .get("metadata")
                .and_then(|m| m.get("receipts"))
                .and_then(|r| r.as_object())
                .map(|receipts| {
                    receipts
                        .keys()
                        .filter_map(|k| TxHash::from_hex(k).ok())
                        .collect()
                })
                .unwrap_or_default();

            if !tx_hashes.is_empty() {
                debug!(
                    "Flashblock diff received with {} tx(s) (index={})",
                    tx_hashes.len(),
                    index
                );
            }

            for tx_hash in tx_hashes {
                if flush_sender
                    .send(FlushRequest::MarkFlashblock {
                        tx_hash,
                        timestamp_ms,
                        index,
                    })
                    .await
                    .is_err()
                {
                    // Actor shut down
                    info!("Flashblocks listener stopping: actor channel closed");
                    return;
                }
            }
        }

        // Reconnect after a short delay
        tokio::time::sleep(Duration::from_secs(1)).await;
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
    pub async fn new<D: DbOps + Send + Sync + 'static>(
        bufsize: usize,
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
        flashblocks_ws_url: Option<Url>,
    ) -> Result<Self> {
        // Pre-flight check: validate flashblocks WS endpoint before spawning tasks
        if let Some(ref ws_url) = flashblocks_ws_url {
            flashblocks_preflight(ws_url).await?;
        }

        let (sender, receiver) = mpsc::channel(bufsize);
        // Channel for flush task to communicate with message handler
        // Larger buffer to accommodate flashblock messages
        let (flush_sender, flush_receiver) = mpsc::channel(64);

        let mut actor = TxActor::new(receiver, flush_receiver, db.clone());

        // Spawn the message handler task (owns the cache)
        tokio::task::spawn(async move {
            if let Err(e) = actor.run().await {
                error!("TxActor message handler terminated with error: {:?}", e);
            }
        });

        // Spawn the independent flush task (communicates via channels)
        let flush_sender_clone = flush_sender.clone();
        tokio::task::spawn(async move {
            flush_loop(flush_sender_clone, db, rpc).await;
        });

        // Spawn the flashblocks listener task if URL is provided
        if let Some(ws_url) = flashblocks_ws_url {
            tokio::task::spawn(async move {
                flashblocks_listener(flush_sender, ws_url).await;
            });
        }

        Ok(Self { sender })
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
