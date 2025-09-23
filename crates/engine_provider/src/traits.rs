use std::time::Duration;

use alloy_rpc_types_engine::ExecutionPayload;
use async_trait::async_trait;
use tracing::warn;

use crate::auth_provider::AuthResult;

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
    async fn replay_chain_segment(&self, start_block: u64) -> AuthResult<ChainReplayResults>;
}

pub trait ControlChain: AdvanceChain + ReplayChain {}

pub trait ToExecutionPayload {
    fn to_payload(&self) -> AuthResult<ExecutionPayload>;
}
