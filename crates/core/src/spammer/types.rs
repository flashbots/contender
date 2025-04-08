use crate::generator::NamedTxRequest;
use alloy::{consensus::TxEnvelope, primitives::FixedBytes};

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
