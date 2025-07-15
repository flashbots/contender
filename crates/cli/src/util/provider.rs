//! Contains a wrapper for auth_provider to handle errors in the cli context.

use async_trait::async_trait;
use contender_engine_provider::{error::AuthProviderError, AdvanceChain, AuthResult};
use tracing::error;

use crate::util::bold;

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
    async fn advance_chain(&self, block_time: u64) -> AuthResult<()> {
        self.auth_provider
            .advance_chain(block_time)
            .await
            .map_err(|e| {
                match &e {
                    AuthProviderError::InternalError(_, err) => {
                        error!("AuthClient encountered an internal error. Please check contender_engine_provider debug logs for more details.");
                        if err.to_string().contains("Invalid newPayload") {
                            error!("You may need to specify a different engine message version with {}", bold("--message-version (-m)"));
                        }
                    }
                    AuthProviderError::ConnectionFailed(_) => {
                        error!("Failed to connect to the auth API. You may need to enable the auth API on your target node.");
                    }
                    AuthProviderError::ExtraDataTooShort => {
                        error!("You may need to remove the {} flag to target this node.", bold("--op"));
                    }
                    AuthProviderError::GasLimitRequired => {
                        error!("You may need to pass the {} flag to target this node.", bold("--op"));
                    }
                }
                e
            })
    }
}
