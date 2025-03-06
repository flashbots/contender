use std::{sync::Arc, time::Duration};

use alloy::{
    network::ReceiptResponse, primitives::TxHash, providers::Provider,
    rpc::types::BlockTransactionsKind,
};
use tokio::sync::{mpsc, oneshot};

use crate::{
    db::{DbOps, RunTx},
    error::ContenderError,
    generator::types::AnyProvider,
};

enum TxActorMessage {
    SentRunTx {
        tx_hash: TxHash,
        start_timestamp: usize,
        kind: Option<String>,
        on_receipt: oneshot::Sender<()>,
    },
    FlushCache {
        run_id: u64,
        on_flush: oneshot::Sender<usize>, // returns the number of txs remaining in cache
        target_block_num: u64,
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
    tx_hash: TxHash,
    start_timestamp: usize,
    kind: Option<String>,
}

impl PendingRunTx {
    pub fn new(tx_hash: TxHash, start_timestamp: usize, kind: Option<&str>) -> Self {
        Self {
            tx_hash,
            start_timestamp,
            kind: kind.map(|s| s.to_owned()),
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

    async fn flush_cache(
        cache: &mut Vec<PendingRunTx>,
        db: &Arc<D>,
        rpc: &Arc<AnyProvider>,
        run_id: u64,
        on_flush: oneshot::Sender<usize>, // returns the number of txs remaining in cache
        target_block_num: u64,
    ) -> Result<Vec<PendingRunTx>, Box<dyn std::error::Error>> {
        println!("unconfirmed txs: {}", cache.len());
        let mut maybe_block;
        loop {
            maybe_block = rpc
                .get_block_by_number(target_block_num.into(), BlockTransactionsKind::Hashes)
                .await;
            if let Ok(maybe_block) = &maybe_block {
                if maybe_block.is_some() {
                    break;
                }
            }
            println!("waiting for block {}", target_block_num);
            std::thread::sleep(Duration::from_secs(1));
        }
        let target_block = maybe_block
            .expect("this should never happen")
            .expect("this should never happen");
        let receipts = rpc
            .get_block_receipts(target_block_num.into())
            .await?
            .unwrap_or_default();
        println!(
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

        // refill cache with any txs that were not included in pending_txs
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
                    println!("tx failed: {:?}", pending_tx.tx_hash);
                } else {
                    println!(
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
                    start_timestamp: pending_tx.start_timestamp / 1000,
                    end_timestamp: target_block.header.timestamp as usize,
                    block_number: target_block.header.number,
                    gas_used: receipt.gas_used,
                    kind: pending_tx.kind,
                }
            })
            .collect::<Vec<_>>();

        db.insert_run_txs(run_id, run_txs)?;
        on_flush
            .send(new_txs.len())
            .map_err(|_| ContenderError::SpamError("failed to join TxActor on_flush", None))?;
        Ok(new_txs)
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
                    ContenderError::SpamError("failed to join TxActor on_stop", None)
                })?;
                return Ok(());
            }
            TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp,
                kind,
                on_receipt,
            } => {
                let run_tx = PendingRunTx {
                    tx_hash,
                    start_timestamp,
                    kind,
                };
                cache.push(run_tx.to_owned());
                on_receipt.send(()).map_err(|_| {
                    ContenderError::SpamError("failed to join TxActor callback", None)
                })?;
            }
            TxActorMessage::FlushCache {
                on_flush,
                run_id,
                target_block_num,
            } => {
                Self::flush_cache(cache, db, rpc, run_id, on_flush, target_block_num).await?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(msg) = self.receiver.recv().await {
            match &msg {
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
                    start_timestamp: _,
                    kind: _,
                    on_receipt: _,
                } => {
                    Self::handle_message(&mut self.cache, &self.db, &self.rpc, msg).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct TxActorHandle {
    sender: mpsc::Sender<TxActorMessage>,
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

    pub async fn cache_run_tx(
        &self,
        tx_hash: TxHash,
        start_timestamp: usize,
        kind: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp,
                kind,
                on_receipt: sender,
            })
            .await?;
        receiver.await?;
        Ok(())
    }

    pub async fn flush_cache(
        &self,
        run_id: u64,
        target_block_num: u64,
    ) -> Result<usize, Box<dyn std::error::Error>> {
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

    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::Stop { on_stop: sender })
            .await?;
        Ok(receiver.await?)
    }
}
