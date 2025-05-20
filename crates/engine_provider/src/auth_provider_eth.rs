use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, Encodable2718},
    network::AnyNetwork,
    primitives::{address, BlockHash, Bytes, FixedBytes, TxKind, B256, U256},
    providers::{ext::EngineApi, DynProvider, Provider, RootProvider},
    rpc::client::ClientBuilder,
    transports::{http::reqwest::Url, TransportResult},
};
use alloy_rpc_types_engine::{ForkchoiceState, ForkchoiceUpdated, JwtSecret, PayloadAttributes};
use async_trait::async_trait;
use op_alloy_consensus::{OpTypedTransaction, TxDeposit};
use op_alloy_network::{primitives::HeaderResponse, BlockResponse, Network, Optimism};
use reth_node_api::EngineApiMessageVersion;
use reth_optimism_node::OpPayloadAttributes;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::{
    engine::Signer,
    read_jwt_file,
    valid_payload::{call_forkchoice_updated, call_new_payload},
};

use super::{auth_transport::AuthenticatedTransportConnect, AdvanceChain};

pub const FJORD_DATA: &[u8] = &alloy::hex!(
    "440a5e200000146b000f79c500000000000000040000000066d052e700000000013ad8a3000000000000000000000000000000000000000000000000000000003ef1278700000000000000000000000000000000000000000000000000000000000000012fdf87b89884a61e74b322bbcf60386f543bfae7827725efaaf0ab1de2294a590000000000000000000000006887246668a3b87f54deb3b94ba47a6f63f32985"
);

#[derive(Clone)]
pub struct AuthProvider<Net = AnyNetwork>
where
    Net: Network,
{
    pub inner: DynProvider<Net>,
    genesis_block: Net::HeaderResponse,
}

pub trait NetworkAttributes {
    type PayloadAttributes: reth_node_api::PayloadAttributes + FcuDefault + Send + Sync + Unpin;
    fn is_op() -> bool;
}

pub trait ProviderExt {
    type PayloadAttributes: reth_node_api::PayloadAttributes + Send + Sync + Unpin;
    fn is_op(&self) -> bool;
}

pub struct OpPayloadParams {
    pub transactions: Option<Vec<Bytes>>,
    pub gas_limit: Option<u64>,
    pub eip_1559_params: Option<FixedBytes<8>>,
}

impl Default for OpPayloadParams {
    fn default() -> Self {
        Self {
            transactions: None,
            gas_limit: None,
            eip_1559_params: None,
        }
    }
}

pub trait FcuDefault: reth_node_api::PayloadAttributes + Send + Sync {
    fn fcu_payload_attributes(timestamp: u64, op_params: Option<OpPayloadParams>) -> Self;
}

impl FcuDefault for OpPayloadAttributes {
    fn fcu_payload_attributes(timestamp: u64, op_params: Option<OpPayloadParams>) -> Self {
        let OpPayloadParams {
            transactions,
            gas_limit,
            eip_1559_params,
        } = op_params.unwrap_or_default();
        OpPayloadAttributes {
            payload_attributes: PayloadAttributes {
                timestamp,
                prev_randao: B256::ZERO,
                suggested_fee_recipient: Default::default(),
                withdrawals: Some(vec![]),
                parent_beacon_block_root: Some(B256::ZERO),
            },
            transactions,
            no_tx_pool: Some(false),
            gas_limit,
            eip_1559_params,
        }
    }
}

impl FcuDefault for PayloadAttributes {
    fn fcu_payload_attributes(timestamp: u64, _op_params: Option<OpPayloadParams>) -> Self {
        PayloadAttributes {
            timestamp,
            prev_randao: B256::ZERO,
            suggested_fee_recipient: Default::default(),
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
        }
    }
}

impl<N> AuthProvider<N>
where
    N: Network + NetworkAttributes,
{
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
        let auth_provider = DynProvider::new(RootProvider::<N>::new(client));
        let genesis_block = auth_provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Earliest)
            .await?
            .expect("no genesis block found")
            .header()
            .to_owned();
        Ok(Self {
            inner: auth_provider,
            genesis_block,
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

    async fn call_fcu_default(
        &self,
        current_head: BlockHash,
        new_head: BlockHash,
        timestamp: Option<u64>,
    ) -> TransportResult<ForkchoiceUpdated> {
        let op_params = if self.is_op() {
            // insert OP block info tx
            let txs = vec![get_op_block_info_tx()
                .await
                .expect("failed to get block info tx")];

            // Parse the last 8 bytes of extra_data as a FixedBytes<8>
            let params = FixedBytes::<8>::from_slice(&self.genesis_block.extra_data()[1..]);
            Some(OpPayloadParams {
                transactions: Some(txs),
                gas_limit: self.genesis_block.gas_limit().into(),
                eip_1559_params: Some(params),
            })
        } else {
            None
        };

        call_forkchoice_updated(
            &self.inner,
            EngineApiMessageVersion::V3,
            ForkchoiceState {
                head_block_hash: new_head,
                safe_block_hash: current_head,
                finalized_block_hash: current_head,
            },
            timestamp.map(|timestamp| {
                N::PayloadAttributes::fcu_payload_attributes(timestamp, op_params)
            }),
        )
        .await
    }
}

#[async_trait]
impl<N: Network + NetworkAttributes> AdvanceChain for AuthProvider<N> {
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
        let header = block.header();
        let res = self
            .call_fcu_default(
                header.hash(),
                header.hash(),
                Some(header.timestamp() + block_time_secs),
            )
            .await?;
        debug!("FCU call sent. Payload ID: {:?}", res.payload_id);
        let payload_id = res.payload_id.expect("need payload ID");

        // wait for builder to build
        if self.is_op() {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

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
        let _res = call_new_payload::<N>(
            &engine_client,
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
        let res = self
            .call_fcu_default(
                block.header().hash(),
                payload
                    .execution_payload
                    .payload_inner
                    .payload_inner
                    .block_hash,
                None,
            )
            .await?;
        debug!("FCU call sent. Payload ID: {:?}", res.payload_id);

        Ok(())
    }
}

impl NetworkAttributes for Optimism {
    type PayloadAttributes = OpPayloadAttributes;
    fn is_op() -> bool {
        true
    }
}

impl NetworkAttributes for AnyNetwork {
    type PayloadAttributes = PayloadAttributes;
    fn is_op() -> bool {
        false
    }
}

impl<N: Network + NetworkAttributes> ProviderExt for AuthProvider<N> {
    fn is_op(&self) -> bool {
        N::is_op()
    }

    type PayloadAttributes = N::PayloadAttributes;
}

async fn get_op_block_info_tx() -> Result<Bytes, Box<dyn std::error::Error>> {
    let deposit_tx = TxDeposit {
        source_hash: B256::default(),
        from: address!("DeaDDEaDDeAdDeAdDEAdDEaddeAddEAdDEAd0001"),
        to: TxKind::Call(address!("4200000000000000000000000000000000000015")),
        mint: None,
        value: U256::default(),
        gas_limit: 210000,
        is_system_transaction: false,
        input: FJORD_DATA.into(),
    };

    // Create a temporary signer for the deposit
    let signer = Signer::random();
    let signed_tx = signer.sign_tx(OpTypedTransaction::Deposit(deposit_tx))?;
    Ok(signed_tx.encoded_2718().into())
}
