pub mod blockwise;
mod spammer_trait;
pub mod timed;
pub mod tx_actor;
mod tx_callback;
pub mod util;

use crate::generator::NamedTxRequest;
use alloy::{consensus::TxEnvelope, primitives::FixedBytes};
pub use blockwise::BlockwiseSpammer;
pub use spammer_trait::Spammer;
pub use timed::TimedSpammer;
pub use tx_callback::{LogCallback, NilCallback, OnTxSent};

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
