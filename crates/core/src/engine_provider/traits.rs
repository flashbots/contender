use async_trait::async_trait;

use crate::engine_provider::auth_provider::AuthResult;

#[async_trait]
pub trait AdvanceChain {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> AuthResult<()>;
}
