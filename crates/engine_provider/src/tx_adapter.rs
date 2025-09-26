use alloy::consensus::Transaction;
use alloy::eips::Encodable2718;
use alloy::primitives::B256;
use alloy::{
    consensus::TxEnvelope,
    primitives::{Bytes, FixedBytes},
    rpc::types::eth,
};
use op_alloy_consensus::OpTxEnvelope;
use op_alloy_network::TransactionResponse;
use op_alloy_rpc_types as op;

use crate::TxLike;

pub enum AnyTx {
    Eth(eth::Transaction),
    Op(op::Transaction),
}

impl From<eth::Transaction> for AnyTx {
    fn from(t: eth::Transaction) -> Self {
        AnyTx::Eth(t)
    }
}
impl From<op::Transaction> for AnyTx {
    fn from(t: op::Transaction) -> Self {
        AnyTx::Op(t)
    }
}

impl TxLike for AnyTx {
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        use AnyTx::*;
        match self {
            Eth(t) => t.blob_versioned_hashes(),
            Op(t) => t.blob_versioned_hashes(),
        }
    }

    fn tx_hash(&self) -> FixedBytes<32> {
        use AnyTx::*;
        match self {
            Eth(t) => {
                // ETH RPC tx implements AsRef<TxEnvelope>
                let env: &TxEnvelope = t.as_ref();
                *env.tx_hash() // via `TxHashRef`
            }
            Op(t) => {
                // OP RPC tx exposes its own hash; use that.
                t.tx_hash()
            }
        }
    }

    fn encoded_2718(&self) -> Bytes {
        use AnyTx::*;
        match self {
            Eth(t) => {
                let env: &TxEnvelope = t.as_ref();
                Bytes::from(env.encoded_2718())
            }
            Op(t) => {
                // OP has deposit txs (non-ETH 2718). For Engine payloads on OP,
                // you still pass the canonical OP tx bytes. OP’s envelope encodes them.
                let env: &OpTxEnvelope = t.as_ref(); // OP RPC tx → OpTxEnvelope
                Bytes::from(env.encoded_2718())
            }
        }
    }
}
