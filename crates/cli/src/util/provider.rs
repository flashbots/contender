//! Contains a wrapper for auth_provider to handle errors in the cli context.

use async_trait::async_trait;
use contender_engine_provider::AdvanceChain;

pub struct AuthClient {
    auth_provider: Box<dyn AdvanceChain + Send + Sync + 'static>,
}

impl AuthClient {
    pub fn new(auth_provider: Box<dyn AdvanceChain + Send + Sync + 'static>) -> Self {
        Self { auth_provider }
    }
}

#[async_trait]
impl AdvanceChain for AuthClient {
    async fn advance_chain(&self, block_time: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.auth_provider
            .advance_chain(block_time)
            .await
            .map_err(|e| {
                if e.to_string().contains("gasLimit parameter is required") {
                    return format!("Failed to advance chain: {e}. You may need to pass the --op flag to target this node.")
                        .into();
                }
                format!(
                    "Failed to advance chain: {e}. Please check your auth provider configuration."
                )
                .into()
            })
    }
}
