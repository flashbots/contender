pub mod blockwise;
pub mod blockwise2;
mod spammer_trait;
pub mod timed;
pub mod tx_actor;
mod tx_callback;
pub mod util;
use crate::generator::NamedTxRequest;
use alloy::consensus::TxEnvelope;
pub use spammer_trait::Spammer;
pub use tx_callback::{LogCallback, NilCallback, OnTxSent};

pub use blockwise::BlockwiseSpammer;
pub use timed::TimedSpammer;

#[derive(Clone, Debug)]
pub enum ExecutionPayload {
    SignedTx(TxEnvelope, NamedTxRequest),
    SignedTxBundle(Vec<TxEnvelope>, Vec<NamedTxRequest>),
}
