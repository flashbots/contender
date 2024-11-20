pub mod blockwise2;
mod spammer_trait;
pub mod timed2;
pub mod tx_actor;
mod tx_callback;
pub mod util;
use crate::generator::NamedTxRequest;
use alloy::{consensus::TxEnvelope, primitives::FixedBytes};
pub use spammer_trait::Spammer;
pub use tx_callback::{LogCallback, NilCallback, OnTxSent};

pub use blockwise2::BlockwiseSpammer2;
pub use timed2::TimedSpammer2;

#[derive(Clone, Debug)]
pub enum ExecutionPayload {
    SignedTx(TxEnvelope, NamedTxRequest),
    SignedTxBundle(Vec<TxEnvelope>, Vec<NamedTxRequest>),
}

#[derive(Clone, Copy, Debug)]
pub enum SpamTrigger {
    Nil,
    BlockNumber(u64),
    Tick(u64),
    BlockHash(FixedBytes<32>),
}
