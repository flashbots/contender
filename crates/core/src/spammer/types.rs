use crate::{
    db::RunTx,
    generator::NamedTxRequest,
    spammer::tx_actor::{PendingRunTx, TxActorMessage},
};
use alloy::{
    consensus::TxEnvelope,
    primitives::{Address, FixedBytes, TxHash},
};
use contender_engine_provider::AuthProviderError;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinError;

#[derive(Debug, Error)]
pub enum CallbackError {
    #[error("auth provider error: {0}")]
    AuthProvider(#[from] AuthProviderError),

    #[error("task failed, join error: {0}")]
    Join(#[from] JoinError),

    // #[error("internal cache write error: {0}")]
    // CacheWrite(String),
    #[error("failed to remove tx from cache: {0}")]
    CacheRemoveTx(TxHash),

    #[error("failed to dump cache. pending txs: {0:?}")]
    DumpCache(Vec<RunTx>),

    #[error("failed to flush cache. pending txs: {0:?}")]
    FlushCache(Vec<PendingRunTx>),

    #[error("failed to send mpsc message: {0}")]
    TxActorSendMessage(#[from] Box<mpsc::error::SendError<TxActorMessage>>),

    #[error("failed to send mpsc message: {0}")]
    MpscSendAddrNonce(#[from] mpsc::error::SendError<(Address, u64)>),

    #[error("oneshot failed to send")]
    OneshotSend(()),

    #[error("oneshot receiver failed: {0}")]
    OneshotReceive(#[from] oneshot::error::RecvError),

    #[error("rpc request failed: {0}")]
    ProviderCall(String),

    #[error("failed to send stop message to TxActor")]
    Stop,

    #[error("{0}")]
    Unknown(String),
}

pub type CallbackResult<T> = Result<T, CallbackError>;

#[derive(Clone, Debug)]
pub enum ExecutionPayload {
    SignedTx(Box<TxEnvelope>, Box<NamedTxRequest>),
    SignedTxBundle(Vec<TxEnvelope>, Vec<NamedTxRequest>),
}

#[derive(Clone, Copy, Debug)]
pub enum SpamTrigger {
    Nil,
    BlockNumber(u64),
    Tick(u64),
    BlockHash(FixedBytes<32>),
}
