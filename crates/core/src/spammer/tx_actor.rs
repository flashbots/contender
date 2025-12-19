use std::{sync::Arc, time::Duration};

use alloy::{network::ReceiptResponse, primitives::TxHash, providers::Provider};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::{
    db::{DbOps, RunTx},
    generator::types::AnyProvider,
    spammer::CallbackError,
    Result,
};

pub enum TxActorMessage {
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
    FlushCache {
        run_id: u64,
        on_flush: oneshot::Sender<Vec<PendingRunTx>>, // returns the number of txs remaining in cache
        target_block_num: u64,
    },
    DumpCache {
        run_id: u64,
        on_dump_cache: oneshot::Sender<Vec<RunTx>>,
    },
    StartAutoFlush {
        run_id: u64,
        flush_interval_blocks: u64,
        on_start: oneshot::Sender<()>,
    },
    StopAutoFlush {
        on_stop_auto: oneshot::Sender<()>,
    },
    Stop {
        on_stop: oneshot::Sender<()>,
    },
}

/// Actor responsible for managing pending transaction cache and processing receipts.
/// 
/// The TxActor runs in a background task and handles:
/// - Caching transaction hashes as they're sent
/// - Automatically flushing confirmed transactions to the database at regular intervals
/// - Manual flush operations on demand
/// - Graceful shutdown
/// 
/// Auto-flush mechanism:
/// - Monitors the blockchain via periodic block number checks
/// - When enough blocks have passed (configurable interval), processes pending transactions
/// - Fetches receipts for transactions in those blocks
/// - Saves confirmed transactions to the database
/// - Retains unconfirmed transactions in cache for next flush
/// 
/// This design allows spamming to continue uninterrupted while receipt processing
/// happens in the background, improving performance and creating more realistic traffic patterns.
struct TxActor<D>
where
    D: DbOps,
{
    receiver: mpsc::Receiver<TxActorMessage>,
    db: Arc<D>,
    /// In-memory cache of pending transactions awaiting confirmation
    cache: Vec<PendingRunTx>,
    rpc: Arc<AnyProvider>,
    /// Whether auto-flush is currently enabled
    auto_flush_enabled: bool,
    /// The run_id to associate with auto-flushed transactions
    auto_flush_run_id: Option<u64>,
    /// Number of blocks between auto-flush operations
    auto_flush_interval_blocks: u64,
    /// Last block number that was successfully flushed
    last_flushed_block: u64,
    /// Track consecutive auto-flush failures to prevent log spam and detect persistent issues
    consecutive_flush_failures: u32,
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

impl<D> TxActor<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(
        receiver: mpsc::Receiver<TxActorMessage>,
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
    ) -> Self {
        Self {
            receiver,
            db,
            cache: Vec::new(),
            rpc,
            auto_flush_enabled: false,
            auto_flush_run_id: None,
            auto_flush_interval_blocks: 10, // default interval
            last_flushed_block: 0,
            consecutive_flush_failures: 0,
        }
    }

    /// Waits for target block to appear onchain,
    /// gets block receipts for the target block,
    /// removes txs that were included in the block from cache, and saves them to the DB.
    async fn flush_cache(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        rpc: &Arc<AnyProvider>,
        run_id: u64,
        on_flush: oneshot::Sender<Vec<PendingRunTx>>, // returns the number of txs remaining in cache
        target_block_num: u64,
    ) -> Result<()> {
        info!("unconfirmed txs: {}", cache.len());

        if cache.is_empty() {
            on_flush
                .send(cache.to_owned())
                .map_err(CallbackError::FlushCache)?;
            return Ok(());
        }

        let mut maybe_block;
        // TODO: replace this garbage mutator thing with a while loop
        loop {
            maybe_block = rpc.get_block_by_number(target_block_num.into()).await;
            if let Ok(maybe_block) = &maybe_block {
                if maybe_block.is_some() {
                    break;
                }
            }
            info!("waiting for block {target_block_num}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let target_block = maybe_block
            .expect("this should never happen")
            .expect("this should never happen");
        let receipts = rpc
            .get_block_receipts(target_block_num.into())
            .await?
            .unwrap_or_default();
        info!(
            "found {} receipts for block {}",
            receipts.len(),
            target_block_num
        );
        // filter for txs that were included in the block
        let receipt_tx_hashes = receipts
            .iter()
            .map(|r| r.transaction_hash)
            .collect::<Vec<_>>();
        let confirmed_txs = cache
            .iter()
            .filter(|tx| receipt_tx_hashes.contains(&tx.tx_hash))
            .map(|tx| tx.to_owned())
            .collect::<Vec<_>>();

        // refill cache with any txs that were not included in confirmed_txs
        let new_txs = cache
            .iter()
            .filter(|tx| !confirmed_txs.contains(tx))
            .map(|tx| tx.to_owned())
            .collect::<Vec<_>>();
        *cache = new_txs.to_vec();

        // ready to go to the DB
        let run_txs = confirmed_txs
            .into_iter()
            .map(|pending_tx| {
                let receipt = receipts
                    .iter()
                    .find(|r| r.transaction_hash == pending_tx.tx_hash)
                    .expect("this should never happen");
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
                            .unwrap_or("N/A".to_owned())
                    );
                }
                RunTx {
                    tx_hash: pending_tx.tx_hash,
                    start_timestamp_secs: (pending_tx.start_timestamp_ms / 1000) as u64,
                    end_timestamp_secs: Some(target_block.header.timestamp),
                    block_number: Some(target_block.header.number),
                    gas_used: Some(receipt.gas_used),
                    kind: pending_tx.kind,
                    error: pending_tx.error,
                }
            })
            .collect::<Vec<_>>();
        db.insert_run_txs(run_id, &run_txs).map_err(|e| e.into())?;
        on_flush
            .send(new_txs.to_owned())
            .map_err(CallbackError::FlushCache)?;
        Ok(())
    }

    /// Dumps all cached txs into the DB. Does not assign `end_timestamp`, `block_number`, or `gas_used`.
    async fn dump_cache(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        run_id: u64,
    ) -> Result<Vec<RunTx>> {
        let run_txs = cache
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
            .collect::<Vec<_>>();
        db.insert_run_txs(run_id, &run_txs).map_err(|e| e.into())?;
        cache.clear();
        Ok(run_txs)
    }

    async fn remove_cached_tx(cache: &mut Vec<PendingRunTx>, old_tx_hash: TxHash) -> Result<()> {
        let old_tx = cache
            .iter()
            .position(|tx| tx.tx_hash == old_tx_hash)
            .ok_or(CallbackError::CacheRemoveTx(old_tx_hash))?;
        cache.remove(old_tx);
        Ok(())
    }

    async fn handle_message(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        rpc: &Arc<AnyProvider>,
        message: TxActorMessage,
        auto_flush_enabled: &mut bool,
        auto_flush_run_id: &mut Option<u64>,
        auto_flush_interval_blocks: &mut u64,
    ) -> Result<()> {
        match message {
            TxActorMessage::Stop { on_stop } => {
                on_stop.send(()).map_err(|_| CallbackError::Stop)?;
                return Ok(());
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
                cache.push(run_tx.to_owned());
                on_receive.send(()).map_err(CallbackError::OneshotSend)?;
            }
            TxActorMessage::RemovedRunTx { tx_hash, on_remove } => {
                Self::remove_cached_tx(cache, tx_hash).await?;
                on_remove.send(()).map_err(CallbackError::OneshotSend)?;
            }
            TxActorMessage::FlushCache {
                on_flush,
                run_id,
                target_block_num,
            } => {
                Self::flush_cache(cache, db, rpc, run_id, on_flush, target_block_num).await?;
            }
            TxActorMessage::DumpCache {
                on_dump_cache,
                run_id,
            } => {
                let res = Self::dump_cache(cache, db, run_id).await?;
                on_dump_cache.send(res).map_err(CallbackError::DumpCache)?;
            }
            TxActorMessage::StartAutoFlush {
                run_id,
                flush_interval_blocks,
                on_start,
            } => {
                *auto_flush_enabled = true;
                *auto_flush_run_id = Some(run_id);
                *auto_flush_interval_blocks = flush_interval_blocks;
                info!("Auto-flush enabled with interval: {} blocks", flush_interval_blocks);
                on_start.send(()).map_err(CallbackError::OneshotSend)?;
            }
            TxActorMessage::StopAutoFlush { on_stop_auto } => {
                *auto_flush_enabled = false;
                *auto_flush_run_id = None;
                info!("Auto-flush disabled");
                on_stop_auto.send(()).map_err(CallbackError::OneshotSend)?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut block_check_interval = tokio::time::interval(Duration::from_secs(1));
        
        loop {
            tokio::select! {
                Some(msg) = self.receiver.recv() => {
                    match &msg {
                        TxActorMessage::DumpCache {
                            on_dump_cache: _,
                            run_id: _,
                        } => {
                            tokio::select! {
                                _ = Self::handle_message(&mut self.cache, &self.db, &self.rpc,
                                    msg, &mut self.auto_flush_enabled, &mut self.auto_flush_run_id,
                                    &mut self.auto_flush_interval_blocks
                                ) => {},
                                Some(TxActorMessage::Stop{on_stop: _}) = self.receiver.recv() => {
                                    // exits early if a stop message is received
                                    break;
                                },
                            };
                        }
                        TxActorMessage::FlushCache {
                            run_id: _,
                            on_flush: _,
                            target_block_num: _,
                        } => {
                            tokio::select! {
                                _ = Self::handle_message(&mut self.cache, &self.db, &self.rpc,
                                    msg, &mut self.auto_flush_enabled, &mut self.auto_flush_run_id,
                                    &mut self.auto_flush_interval_blocks
                                ) => {},
                                Some(TxActorMessage::Stop{on_stop: _}) = self.receiver.recv() => {
                                    // exits early if a stop message is received
                                    break;
                                },
                            };
                        }
                        TxActorMessage::SentRunTx {
                            tx_hash: _,
                            start_timestamp_ms: _,
                            kind: _,
                            on_receive: _,
                            error: _,
                        } => {
                            Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg,
                                &mut self.auto_flush_enabled, &mut self.auto_flush_run_id,
                                &mut self.auto_flush_interval_blocks).await?;
                        }
                        TxActorMessage::RemovedRunTx {
                            tx_hash: _,
                            on_remove: _,
                        } => {
                            Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg,
                                &mut self.auto_flush_enabled, &mut self.auto_flush_run_id,
                                &mut self.auto_flush_interval_blocks).await?;
                        }
                        TxActorMessage::StartAutoFlush { .. } | TxActorMessage::StopAutoFlush { .. } => {
                            Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg,
                                &mut self.auto_flush_enabled, &mut self.auto_flush_run_id,
                                &mut self.auto_flush_interval_blocks).await?;
                        }
                        TxActorMessage::Stop { on_stop: _ } => {
                            // do nothing here; stop is a signal to interrupt other message handlers
                            break;
                        }
                    }
                },
                _ = block_check_interval.tick() => {
                    if self.auto_flush_enabled && !self.cache.is_empty() {
                        if let Some(run_id) = self.auto_flush_run_id {
                            // Check current block number
                            match self.rpc.get_block_number().await {
                                Ok(current_block) => {
                                    // Initialize last_flushed_block if this is the first check
                                    if self.last_flushed_block == 0 {
                                        self.last_flushed_block = current_block.saturating_sub(1);
                                    }
                                    
                                    // Flush if enough blocks have passed
                                    if current_block >= self.last_flushed_block + self.auto_flush_interval_blocks {
                                        let target_block = self.last_flushed_block + 1;
                                        
                                        // Only log periodically to avoid spam during failures
                                        if self.consecutive_flush_failures == 0 {
                                            debug!("Auto-flushing cache for block {} (cache size: {})", target_block, self.cache.len());
                                        }
                                        
                                        // Create a dummy oneshot channel since we don't need the response
                                        let (tx, _rx) = oneshot::channel();
                                        match Self::flush_cache(
                                            &mut self.cache,
                                            &self.db,
                                            &self.rpc,
                                            run_id,
                                            tx,
                                            target_block,
                                        ).await {
                                            Ok(_) => {
                                                self.last_flushed_block = target_block;
                                                // Reset failure counter on success
                                                if self.consecutive_flush_failures > 0 {
                                                    info!("Auto-flush recovered after {} failures", self.consecutive_flush_failures);
                                                    self.consecutive_flush_failures = 0;
                                                }
                                            },
                                            Err(e) => {
                                                self.consecutive_flush_failures += 1;
                                                // Log every 10 failures to avoid spam
                                                if self.consecutive_flush_failures == 1 || self.consecutive_flush_failures % 10 == 0 {
                                                    warn!(
                                                        "Auto-flush failed (attempt {}): {:?}. Cache size: {}. Will retry.",
                                                        self.consecutive_flush_failures, e, self.cache.len()
                                                    );
                                                }
                                            }
                                        }
                                    }
                                },
                                Err(e) => {
                                    if self.consecutive_flush_failures == 0 {
                                        warn!("Failed to get block number for auto-flush: {:?}. Will retry.", e);
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
        Ok(())
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
        let mut actor = TxActor::new(receiver, db, rpc);
        tokio::task::spawn(async move {
            actor.run().await.expect("tx actor massively failed");
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

    /// Removes txs included onchain from the cache, saves them to the DB, and returns the number of txs remaining in the cache.
    pub async fn flush_cache(
        &self,
        run_id: u64,
        target_block_num: u64,
    ) -> Result<Vec<PendingRunTx>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::FlushCache {
                run_id,
                on_flush: sender,
                target_block_num,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
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

    /// Starts automatic cache flushing in the background.
    /// 
    /// Once started, the TxActor will automatically flush pending transactions
    /// every `flush_interval_blocks` blocks. This allows spam operations to continue
    /// uninterrupted while receipts are collected and saved to the database.
    /// 
    /// # Arguments
    /// * `run_id` - The database run ID to associate with flushed transactions
    /// * `flush_interval_blocks` - Number of blocks between flush operations
    /// 
    /// # Returns
    /// Ok(()) if auto-flush was successfully started
    /// 
    /// # Notes
    /// - Auto-flush will continue until explicitly stopped via `stop_auto_flush()`
    /// - Failures are automatically retried on the next interval
    /// - Multiple calls to start_auto_flush will update the configuration
    pub async fn start_auto_flush(
        &self,
        run_id: u64,
        flush_interval_blocks: u64,
    ) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::StartAutoFlush {
                run_id,
                flush_interval_blocks,
                on_start: sender,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    /// Stops automatic cache flushing.
    /// 
    /// After calling this, the TxActor will no longer automatically flush
    /// pending transactions. Any remaining transactions in the cache can be
    /// flushed manually via `flush_cache()` or dumped via `dump_cache()`.
    /// 
    /// # Returns
    /// Ok(()) if auto-flush was successfully stopped
    /// 
    /// # Notes
    /// - Typically called before final manual flush at end of spam run
    /// - Does not affect transactions already in the cache
    pub async fn stop_auto_flush(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::StopAutoFlush {
                on_stop_auto: sender,
            })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }

    /// Stops the actor, terminating any pending tasks.
    pub async fn stop(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::Stop { on_stop: sender })
            .await
            .map_err(Box::new)
            .map_err(CallbackError::from)?;
        Ok(receiver.await.map_err(CallbackError::OneshotReceive)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_run_tx_creation() {
        let tx_hash = TxHash::default();
        let timestamp = 1234567890u128;
        let kind = Some("test");
        let error = Some("error message");

        let pending_tx = PendingRunTx::new(tx_hash, timestamp, kind, error);

        assert_eq!(pending_tx.tx_hash, tx_hash);
        assert_eq!(pending_tx.start_timestamp_ms, timestamp);
        assert_eq!(pending_tx.kind, Some("test".to_string()));
        assert_eq!(pending_tx.error, Some("error message".to_string()));
    }

    #[test]
    fn test_pending_run_tx_no_kind_or_error() {
        let tx_hash = TxHash::default();
        let timestamp = 1234567890u128;

        let pending_tx = PendingRunTx::new(tx_hash, timestamp, None, None);

        assert_eq!(pending_tx.tx_hash, tx_hash);
        assert_eq!(pending_tx.start_timestamp_ms, timestamp);
        assert_eq!(pending_tx.kind, None);
        assert_eq!(pending_tx.error, None);
    }

    #[test]
    fn test_cache_tx_creation() {
        let tx_hash = TxHash::default();
        let timestamp = 9876543210u128;
        let kind = Some("transfer".to_string());
        let error = None;

        let cache_tx = CacheTx {
            tx_hash,
            start_timestamp_ms: timestamp,
            kind: kind.clone(),
            error: error.clone(),
        };

        assert_eq!(cache_tx.tx_hash, tx_hash);
        assert_eq!(cache_tx.start_timestamp_ms, timestamp);
        assert_eq!(cache_tx.kind, kind);
        assert_eq!(cache_tx.error, error);
    }
}
