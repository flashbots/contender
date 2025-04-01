use std::path::PathBuf;

use crate::eth_engine::{
    auth_transport::AuthenticatedTransportConnect,
    valid_payload::{call_fcu_default, call_new_payload},
};
use crate::{error::ContenderError, generator::types::AnyProvider};
use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::B256,
    providers::{ext::EngineApi, DynProvider, Provider, RootProvider},
    rpc::client::ClientBuilder,
    transports::http::reqwest::Url,
};
use alloy_rpc_types_engine::JwtSecret;

pub const DEFAULT_BLOCK_TIME: u64 = 1;

pub async fn get_auth_provider(
    auth_rpc_url: &str,
    jwt_secret: PathBuf,
) -> Result<AnyProvider, Box<dyn std::error::Error>> {
    // parse url from engine args
    let auth_url = Url::parse(auth_rpc_url).expect("Invalid auth RPC URL");

    // fetch jwt from file
    //
    // the jwt is hex encoded so we will decode it after
    if !jwt_secret.is_file() {
        return Err(ContenderError::GenericError(
            "JWT secret file not found:",
            jwt_secret.to_string_lossy().into(),
        )
        .into());
    }
    let jwt = std::fs::read_to_string(jwt_secret)?;
    let jwt = JwtSecret::from_hex(jwt)?;

    let auth_transport = AuthenticatedTransportConnect::new(auth_url, jwt);
    let client = ClientBuilder::default()
        .connect_with(auth_transport)
        .await?;
    let auth_provider = RootProvider::<AnyNetwork>::new(client);
    Ok(DynProvider::new(auth_provider))
}

/// Advances a chain by one block. `engine_client` must be authenticated with a JWT token.
pub async fn advance_chain(
    engine_client: &AnyProvider,
    block_time_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("advancing chain...");

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
    println!("FCU call sent. Payload ID: {:?}", res.payload_id);
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
    println!("new payload sent.");

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
    println!("FCU call sent. Payload ID: {:?}", res.payload_id);

    Ok(())
}
