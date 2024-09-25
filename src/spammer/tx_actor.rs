use std::sync::Arc;

use alloy::primitives::TxHash;
use tokio::sync::{mpsc, oneshot};

use crate::db::database::{DbOps, RunTx};

enum TxActorMessage {
    SentRunTx {
        tx_hash: TxHash,
        start_timestamp: usize,
        end_timestamp: usize,
        block_number: u64,
        gas_used: u128,
        on_receipt: oneshot::Sender<RunTx>,
    },
    FlushCache {
        run_id: u64,
        on_flush: oneshot::Sender<()>,
    },
}

struct TxActor<D>
where
    D: DbOps,
{
    receiver: mpsc::Receiver<TxActorMessage>,
    db: Arc<D>,
    cache: Vec<RunTx>,
}

impl<D> TxActor<D>
where
    D: DbOps + Send + Sync + 'static,
{
    pub fn new(receiver: mpsc::Receiver<TxActorMessage>, db: Arc<D>) -> Self {
        Self {
            receiver,
            db: db.clone(),
            cache: Vec::new(),
        }
    }

    fn handle_message(&mut self, message: TxActorMessage) {
        match message {
            TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp,
                end_timestamp,
                block_number,
                gas_used,
                on_receipt,
            } => {
                let run_tx = RunTx {
                    tx_hash,
                    start_timestamp,
                    end_timestamp,
                    block_number,
                    gas_used,
                };
                self.cache.push(run_tx.to_owned());
                on_receipt.send(run_tx).unwrap();
            }
            TxActorMessage::FlushCache { on_flush, run_id } => {
                let run_txs = self.cache.drain(..).collect::<Vec<_>>();
                self.db.insert_run_txs(run_id, run_txs).unwrap();
                on_flush.send(()).unwrap();
            }
        }
    }

    pub async fn run(&mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg);
        }
    }
}

pub struct TxActorHandle {
    sender: mpsc::Sender<TxActorMessage>,
}

impl TxActorHandle {
    pub fn new<D: DbOps + Send + Sync + 'static>(bufsize: usize, db: Arc<D>) -> Self {
        let (sender, receiver) = mpsc::channel(bufsize);
        let mut actor = TxActor::new(receiver, db);
        tokio::task::spawn(async move {
            actor.run().await;
        });
        Self { sender }
    }

    pub async fn cache_run_tx(
        &self,
        tx_hash: TxHash,
        start_timestamp: usize,
        end_timestamp: usize,
        block_number: u64,
        gas_used: u128,
    ) {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::SentRunTx {
                tx_hash,
                start_timestamp,
                end_timestamp,
                block_number,
                gas_used,
                on_receipt: sender,
            })
            .await
            .unwrap();
        receiver.await.unwrap();
    }

    pub async fn flush_cache(&self, run_id: u64) {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(TxActorMessage::FlushCache {
                run_id,
                on_flush: sender,
            })
            .await
            .unwrap();
        receiver.await.unwrap()
    }
}
