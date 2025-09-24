use std::time::Duration;

use alloy::{
    consensus::{transaction::TxHashRef, Transaction},
    primitives::Bytes,
};
use alloy_rpc_types_engine::ExecutionPayload;
use async_trait::async_trait;
use tracing::warn;

use crate::auth_provider::{AuthResult, OpPayloadParams};

#[async_trait]
pub trait AdvanceChain {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> AuthResult<()>;
}

#[derive(Clone, Debug)]
pub struct ChainReplayResults {
    pub gas_used: u128,
    pub time_elapsed: Duration,
}

impl ChainReplayResults {
    /// Returns the average execution speed in gas/second.
    pub fn gas_per_second(&self) -> u128 {
        if self.gas_used == 0 {
            warn!("no gas was used; the block may be empty.");
        }
        self.gas_used / self.time_elapsed.as_secs().max(1) as u128
    }
}

#[async_trait]
pub trait ReplayChain {
    /// Re-send & re-validate a range of previously-committed blocks.
    /// Returns relevant results from the replay execution.
    async fn replay_chain_segment(
        &self,
        start_block: u64,
        end_block: Option<u64>,
    ) -> AuthResult<ChainReplayResults>;
}

pub trait ControlChain: AdvanceChain + ReplayChain {}

#[async_trait]
pub trait BlockToPayload {
    async fn block_to_payload(&self, block_num: u64) -> AuthResult<ExecutionPayload>;
}

pub trait TxEnvelopeTransformer {
    fn to_envelope(&self) -> impl Transaction + TxHashRef;
}

pub trait FcuDefault: reth_node_api::PayloadAttributes + Send + Sync {
    fn fcu_payload_attributes(timestamp: u64, op_params: Option<OpPayloadParams>) -> Self;
}

pub trait DefaultTxEncoding {
    type Tx;
    fn encode_tx(tx: Self::Tx) -> Bytes;
}
