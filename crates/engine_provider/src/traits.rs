use async_trait::async_trait;

use crate::ProviderExt;

#[async_trait]
pub trait AdvanceChain: ProviderExt {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> Result<(), Box<dyn std::error::Error>>;
}
