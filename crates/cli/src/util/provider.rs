//! Contains a wrapper for auth_provider to handle errors in the cli context.

use async_trait::async_trait;
use contender_engine_provider::{error::AuthProviderError, AdvanceChain, AuthResult};
use tracing::warn;

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
                match e {
                    AuthProviderError::InternalError(_, _) => {
                        warn!("AuthClient encountered an internal error. Please check contender_engine_provider debug logs for more details.");
                    }
                    AuthProviderError::ConnectionFailed(_) => {
                        warn!("Please check the auth provider connection.");
                    }
                    AuthProviderError::ExtraDataTooShort => {
                        warn!("You may need to remove the --op flag to target this node.");
                    }
                    AuthProviderError::GasLimitRequired => {
                        warn!("You may need to pass the --op flag to target this node.");
                    }
                }
                e
            })
    }
}
