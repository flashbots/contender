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

struct TxActor<D>
where
    D: DbOps,
{
    receiver: mpsc::Receiver<TxActorMessage>,
    db: Arc<D>,
    cache: Vec<PendingRunTx>,
    rpc: Arc<AnyProvider>,
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
}

impl ActorContext {
    pub fn new(target_block: u64, run_id: u64) -> Self {
        Self {
            run_id,
            target_block,
        }
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
        db: Arc<D>,
        rpc: Arc<AnyProvider>,
    ) -> Self {
        Self {
            receiver,
            db,
            cache: Vec::new(),
            rpc,
            ctx: None,
            status: ActorStatus::default(),
        }
    }

    pub fn update_ctx_target_block(&mut self, target_block_num: u64) -> Result<()> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.target_block = target_block_num;
        } else {
            return Err(CallbackError::UpdateRequiresCtx.into());
        }

        Ok(())
    }

    /// Waits for target block to appear onchain,
    /// gets block receipts for the target block,
    /// removes txs that were included in the block from cache, and saves them to the DB.
    async fn flush_cache(
        &mut self,
        run_id: u64,
        on_flush: oneshot::Sender<Vec<PendingRunTx>>, // returns the number of txs remaining in cache
        target_block_num: u64,
    ) -> Result<()> {
        let Self { cache, rpc, db, .. } = self;
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
    async fn dump_cache(&mut self, run_id: u64) -> Result<Vec<RunTx>> {
        let run_txs = self
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
            .collect::<Vec<_>>();
        self.db
            .insert_run_txs(run_id, &run_txs)
            .map_err(|e| e.into())?;
        self.cache.clear();
        Ok(run_txs)
    }

    async fn remove_cached_tx(&mut self, old_tx_hash: TxHash) -> Result<()> {
        let old_tx = self
            .cache
            .iter()
            .position(|tx| tx.tx_hash == old_tx_hash)
            .ok_or(CallbackError::CacheRemoveTx(old_tx_hash))?;
        self.cache.remove(old_tx);
        Ok(())
    }

    /// Parse message and execute appropriate methods.
    async fn handle_message(&mut self, message: TxActorMessage) -> Result<()> {
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
                self.cache.push(run_tx.to_owned());
                on_receive
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("SentRunTx: {:?}", e)))?;
            }
            TxActorMessage::RemovedRunTx { tx_hash, on_remove } => {
                self.remove_cached_tx(tx_hash).await?;
                on_remove
                    .send(())
                    .map_err(|e| CallbackError::OneshotSend(format!("RemovedRunTx: {:?}", e)))?;
            }
            TxActorMessage::DumpCache {
                on_dump_cache,
                run_id,
            } => {
                let res = self.dump_cache(run_id).await?;
                on_dump_cache.send(res).map_err(CallbackError::DumpCache)?;
            }
        }

        Ok(())
    }

    /// Receive & handle messages.
    pub async fn run(&mut self) -> Result<()> {
        let mut interval =
            tokio::time::interval(/* self.cfg.poll_interval */ Duration::from_secs(1));
        let provider = self.rpc.clone();

        loop {
            if self.is_shutting_down() {
                break;
            }

            tokio::select! {
                // periodically flush cache in background while spammer runs
                _ = interval.tick() => {
                    if let Some(ctx) = self.ctx.to_owned() {
                        let new_block = provider.get_block_number().await?;
                        for bn in ctx.target_block..new_block {
                            let (on_flush, _receiver) = oneshot::channel();
                            self.flush_cache(ctx.run_id, on_flush, bn).await?;
                        }
                        self.update_ctx_target_block(new_block)?;
                    } else {
                        debug!("TxActor context not initialized.");
                    }
                }

                // handle messages (sent by test_scenario)
                msg = self.receiver.recv() => {
                    if let Some(msg) = msg {
                        self.handle_message(msg).await?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl<D: DbOps> TxActor<D> {
    pub fn is_shutting_down(&self) -> bool {
        self.status == ActorStatus::ShuttingDown
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
            actor.run().await?;
            Ok::<_, crate::Error>(())
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
