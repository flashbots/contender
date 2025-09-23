use alloy_rpc_types_engine::ExecutionPayload;
use async_trait::async_trait;

use crate::auth_provider::AuthResult;

#[async_trait]
pub trait AdvanceChain {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> AuthResult<()>;
}

#[async_trait]
pub trait ReplayChain {
    /// Re-send & re-validate a range of previously-committed blocks.
    async fn replay_chain_segment(&self, start_block: u64) -> AuthResult<()>;
}

pub trait ControlChain: AdvanceChain + ReplayChain {}

pub trait ToExecutionPayload {
    fn to_payload(&self) -> AuthResult<ExecutionPayload>;
}
