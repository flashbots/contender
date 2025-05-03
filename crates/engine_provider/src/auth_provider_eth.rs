use std::path::PathBuf;

use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::B256,
    providers::{ext::EngineApi, DynProvider, Provider, RootProvider},
    rpc::client::ClientBuilder,
    transports::http::reqwest::Url,
};
use alloy_rpc_types_engine::JwtSecret;
use async_trait::async_trait;
use tracing::info;

use crate::{
    read_jwt_file,
    valid_payload::{call_fcu_default, call_new_payload},
};

use super::{auth_transport::AuthenticatedTransportConnect, AdvanceChain};

#[derive(Clone)]
pub struct AuthProvider {
    inner: DynProvider<AnyNetwork>,
}

impl AuthProvider {
    /// Create a new AuthProvider instance.
    /// This will create a new authenticated transport connected to `auth_rpc_url` using `jwt_secret`.
    pub async fn new(
        auth_rpc_url: &str,
        jwt_secret: JwtSecret,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let auth_url = Url::parse(auth_rpc_url).expect("Invalid auth RPC URL");
        let auth_transport = AuthenticatedTransportConnect::new(auth_url, jwt_secret);
        let client = ClientBuilder::default()
            .connect_with(auth_transport)
            .await?;
        let auth_provider = RootProvider::<AnyNetwork>::new(client);
        Ok(Self {
            inner: DynProvider::new(auth_provider),
        })
    }

    /// Create a new AuthProvider instance from a JWT secret file.
    /// The JWT secret is hex encoded and will be decoded after reading the file.
    pub async fn from_jwt_file(
        auth_rpc_url: &str,
        jwt_secret_file: &PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // fetch jwt from file
        let jwt = read_jwt_file(jwt_secret_file)?;
        Self::new(auth_rpc_url, jwt).await
    }
}

#[async_trait]
impl AdvanceChain for AuthProvider {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
        info!("advancing chain...");
        let engine_client = &self.inner;

        let block = engine_client
            .get_block(BlockId::latest())
            .full()
            .await?
            .expect("no block found");

        //
        // first FCU: call with same hash for parent and new head
        //
        let res = call_fcu_default(
            engine_client,
            block.header.hash,
            block.header.hash,
            Some(block.header.timestamp + block_time_secs),
        )
        .await?;
        info!("FCU call sent. Payload ID: {:?}", res.payload_id);
        let payload_id = res.payload_id.expect("need payload ID");

        //
        // getPayload with new payload ID
        //
        let payload = engine_client
            .get_payload_v3(payload_id)
            .await
            .expect("failed to call getPayload");

        //
        // newPayload with fresh payload from target
        //
        let _res = call_new_payload(
            engine_client,
            payload.execution_payload.to_owned().into(),
            Some(B256::ZERO),
            vec![],
        )
        .await
        .expect("failed to call newPayload");
        info!("new payload sent.");

        //
        // second FCU: call with updated block head from new payload
        //
        let res = call_fcu_default(
            engine_client,
            block.header.hash,
            payload
                .execution_payload
                .payload_inner
                .payload_inner
                .block_hash,
            None,
        )
        .await?;
        info!("FCU call sent. Payload ID: {:?}", res.payload_id);

        Ok(())
    }
}
