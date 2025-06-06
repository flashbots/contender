use std::{sync::Arc, time::Duration};

use alloy::{network::ReceiptResponse, primitives::TxHash, providers::Provider};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

use crate::{
    db::{DbOps, RunTx},
    error::ContenderError,
    generator::types::AnyProvider,
};

enum TxActorMessage {
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
    Stop {
        on_stop: oneshot::Sender<()>,
    },
}

struct TxActor<D>
where
    D: DbOps,
{
    receiver: mpsc::Receiver<TxActorMessage>,
    db: Arc<D>,
    cache: Vec<PendingRunTx>,
    rpc: Arc<AnyProvider>,
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("unconfirmed txs: {}", cache.len());

        if cache.is_empty() {
            on_flush
                .send(cache.to_owned())
                .map_err(|_| ContenderError::SpamError("failed to join TxActor on_flush", None))?;
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
        db.insert_run_txs(run_id, &run_txs)?;
        on_flush
            .send(new_txs.to_owned())
            .map_err(|_| ContenderError::SpamError("failed to join TxActor on_flush", None))?;
        Ok(())
    }

    /// Dumps all cached txs into the DB. Does not assign `end_timestamp`, `block_number`, or `gas_used`.
    async fn dump_cache(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        run_id: u64,
    ) -> Result<Vec<RunTx>, Box<dyn std::error::Error>> {
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
        db.insert_run_txs(run_id, &run_txs)?;
        cache.clear();
        Ok(run_txs)
    }

    async fn remove_cached_tx(
        cache: &mut Vec<PendingRunTx>,
        old_tx_hash: TxHash,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let old_tx = cache
            .iter()
            .position(|tx| tx.tx_hash == old_tx_hash)
            .ok_or(ContenderError::SpamError(
                "failed to find tx in cache to replace",
                None,
            ))?;
        cache.remove(old_tx);
        Ok(())
    }

    async fn handle_message(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        rpc: &Arc<AnyProvider>,
        message: TxActorMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match message {
            TxActorMessage::Stop { on_stop } => {
                on_stop.send(()).map_err(|_| {
                    ContenderError::SpamError("failed to join TxActor on_stop (Stop)", None)
                })?;
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
                on_receive.send(()).map_err(|_| {
                    ContenderError::SpamError("failed to join TxActor on_receive (SentRunTx)", None)
                })?;
            }
            TxActorMessage::RemovedRunTx { tx_hash, on_remove } => {
                Self::remove_cached_tx(cache, tx_hash).await?;
                on_remove.send(()).map_err(|_| {
                    ContenderError::SpamError(
                        "failed to join TxActor on_replace (ReplacedRunTx)",
                        None,
                    )
                })?;
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
                on_dump_cache.send(res).map_err(|_| {
                    ContenderError::SpamError(
                        "failed to join TxActor on_get_cache (FlushCache)",
                        None,
                    )
                })?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(msg) = self.receiver.recv().await {
            match &msg {
                TxActorMessage::DumpCache {
                    on_dump_cache: _,
                    run_id: _,
                } => {
                    tokio::select! {
                        _ = Self::handle_message(&mut self.cache, &self.db, &self.rpc,
                            msg
                        ) => {},
                        Some(TxActorMessage::Stop{on_stop: _}) = self.receiver.recv() => {
                            // exits early if a stop message is received
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
                            msg
                        ) => {},
                        Some(TxActorMessage::Stop{on_stop: _}) = self.receiver.recv() => {
                            // exits early if a stop message is received
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
                    Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg).await?;
                }
                TxActorMessage::RemovedRunTx {
                    tx_hash: _,
                    on_remove: _,
                } => {
                    Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg).await?;
                }
                TxActorMessage::Stop { on_stop: _ } => {
                    // do nothing here; stop is a signal to interrupt other message handlers
                }
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
    pub async fn cache_run_tx(&self, params: CacheTx) -> Result<(), Box<dyn std::error::Error>> {
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
            .await?;
        receiver.await?;
        Ok(())
    }

    /// Dumps remaining txs in cache to the DB and returns them. Does not assign `end_timestamp`, `block_number`, or `gas_used`.
    pub async fn dump_cache(&self, run_id: u64) -> Result<Vec<RunTx>, Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::DumpCache {
                on_dump_cache: sender,
                run_id,
            })
            .await?;
        Ok(receiver.await?)
    }

    /// Removes txs included onchain from the cache, saves them to the DB, and returns the number of txs remaining in the cache.
    pub async fn flush_cache(
        &self,
        run_id: u64,
        target_block_num: u64,
    ) -> Result<Vec<PendingRunTx>, Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::FlushCache {
                run_id,
                on_flush: sender,
                target_block_num,
            })
            .await?;
        Ok(receiver.await?)
    }

    /// Removes an existing tx in the cache.
    pub async fn remove_cached_tx(
        &self,
        tx_hash: TxHash,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::RemovedRunTx {
                tx_hash,
                on_remove: sender,
            })
            .await?;

        Ok(receiver.await?)
    }

    /// Stops the actor, terminating any pending tasks.
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::Stop { on_stop: sender })
            .await?;
        Ok(receiver.await?)
    }
}
