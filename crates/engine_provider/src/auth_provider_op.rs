use crate::{util::read_jwt_file, AdvanceChain};
use alloy::hex::ToHexExt;
use op_rbuilder::tester::{EngineApi, EngineApiBuilder};
use std::path::PathBuf;

pub struct AuthProviderOp {
    engine_client: EngineApi,
}

impl AuthProviderOp {
    /// Create a new AuthProvider instance.
    /// This will create a new authenticated transport connected to `auth_rpc_url` using `jwt_secret`.
    pub async fn new(
        auth_rpc_url: &str,
        jwt_secret: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let engine_client = EngineApiBuilder::new()
            .with_jwt_secret(jwt_secret)
            .with_url(auth_rpc_url)
            .build()?;
        Ok(Self { engine_client })
    }

    /// Create a new AuthProvider instance from a JWT secret file.
    /// The JWT secret is hex encoded and will be decoded after reading the file.
    pub async fn from_jwt_file(
        auth_rpc_url: &str,
        jwt_secret_file: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // fetch jwt from file
        let jwt = read_jwt_file(jwt_secret_file)?;
        let jwt_hex = jwt.as_bytes().encode_hex();
        Self::new(auth_rpc_url, &jwt_hex).await
    }
}

#[async_trait::async_trait]
impl AdvanceChain for AuthProviderOp {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
        println!("[OP] advancing chain {block_time_secs}s...");
        self.engine_client.latest().await?;
        todo!();
    }
}
