use super::{auth_transport::AuthenticatedTransportConnect, AdvanceChain};
use crate::tx_adapter::AnyTx;
use crate::{engine::Signer, read_jwt_file};
use crate::{error::AuthProviderError, valid_payload::EngineApiValidWaitExt};
use crate::{
    BlockToPayload, ChainReplayResults, ControlChain, FcuDefault, GetBlockTxs, ReplayChain, RpcId,
    TxLike,
};
use alloy::consensus::BlobTransactionSidecar;
use alloy::eips::eip7685::RequestsOrHash;
use alloy::primitives::map::HashMap;
use alloy::signers::k256::sha2::Digest;
use alloy::signers::k256::sha2::Sha256;
use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, Encodable2718},
    network::AnyNetwork,
    primitives::{address, BlockHash, Bytes, FixedBytes, TxKind, B256, U256},
    providers::{ext::EngineApi, Provider, RootProvider},
    rpc::client::ClientBuilder,
    transports::{http::reqwest::Url, TransportResult},
};
use alloy_rpc_types_engine::{
    ExecutionPayload, ExecutionPayloadInputV2, ExecutionPayloadV1, ExecutionPayloadV2,
    ExecutionPayloadV3, ForkchoiceState, ForkchoiceUpdated, JwtSecret, PayloadAttributes,
};
use async_trait::async_trait;
use op_alloy_consensus::{OpTypedTransaction, TxDeposit};
use op_alloy_network::Ethereum;
use op_alloy_network::{primitives::HeaderResponse, BlockResponse, Network, Optimism};
use reth_node_api::EngineApiMessageVersion;
use reth_optimism_node::OpPayloadAttributes;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info};

pub const FJORD_DATA: &[u8] = &alloy::hex!(
    "440a5e200000146b000f79c500000000000000040000000066d052e700000000013ad8a3000000000000000000000000000000000000000000000000000000003ef1278700000000000000000000000000000000000000000000000000000000000000012fdf87b89884a61e74b322bbcf60386f543bfae7827725efaaf0ab1de2294a590000000000000000000000006887246668a3b87f54deb3b94ba47a6f63f32985"
);

pub type AuthResult<T> = std::result::Result<T, AuthProviderError>;

#[derive(Clone)]
pub struct AuthProvider<Net = AnyNetwork>
where
    Net: Network,
{
    pub inner: RootProvider<Net>,
    genesis_block: Net::HeaderResponse,
    pub message_version: EngineApiMessageVersion,
    pub rpc_url: String,
}

pub trait NetworkAttributes {
    type PayloadAttributes: reth_node_api::PayloadAttributes + FcuDefault + Send + Sync + Unpin;
    fn is_op() -> bool;
}

pub trait ProviderExt {
    type PayloadAttributes: reth_node_api::PayloadAttributes + Send + Sync + Unpin;
    fn is_op(&self) -> bool;
}

#[derive(Default)]
pub struct OpPayloadParams {
    pub transactions: Option<Vec<Bytes>>,
    pub gas_limit: Option<u64>,
    pub eip_1559_params: Option<FixedBytes<8>>,
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
        message_version: EngineApiMessageVersion,
    ) -> AuthResult<Self> {
        let auth_url = Url::parse(auth_rpc_url).expect("Invalid auth RPC URL");
        let auth_transport = AuthenticatedTransportConnect::new(auth_url, jwt_secret);
        let client = ClientBuilder::default()
            .connect_with(auth_transport)
            .await
            .map_err(|e| AuthProviderError::ConnectionFailed(e.into()))?;
        let auth_provider = RootProvider::<N>::new(client);
        let genesis_block = auth_provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Earliest)
            .await
            .map_err(|e| AuthProviderError::Internal(format!("failed to get genesis block: {e}")))?
            .expect("no genesis block found")
            .header()
            .to_owned();
        Ok(Self {
            inner: auth_provider,
            genesis_block,
            message_version,
            rpc_url: auth_rpc_url.to_owned(),
        })
    }

    /// Create a new AuthProvider instance from a JWT secret file.
    /// The JWT secret is hex encoded and will be decoded after reading the file.
    pub async fn from_jwt_file(
        auth_rpc_url: &str,
        jwt_secret_file: &PathBuf,
        message_version: EngineApiMessageVersion,
    ) -> AuthResult<Self> {
        // fetch jwt from file
        let jwt = read_jwt_file(jwt_secret_file)
            .map_err(|e| AuthProviderError::Internal(format!("failed to read jwt file: {e}")))?;
        Self::new(auth_rpc_url, jwt, message_version).await
    }

    pub async fn call_forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        match self.message_version {
            EngineApiMessageVersion::V5 => todo!("V5 payloads not supported yet"),
            EngineApiMessageVersion::V4 => {
                self.inner
                    .fork_choice_updated_v3_wait(forkchoice_state, payload_attributes)
                    .await
            }
            EngineApiMessageVersion::V3 => {
                self.inner
                    .fork_choice_updated_v3_wait(forkchoice_state, payload_attributes)
                    .await
            }
            EngineApiMessageVersion::V2 => {
                self.inner
                    .fork_choice_updated_v2_wait(forkchoice_state, payload_attributes)
                    .await
            }
            EngineApiMessageVersion::V1 => {
                self.inner
                    .fork_choice_updated_v1_wait(forkchoice_state, payload_attributes)
                    .await
            }
        }
    }

    async fn call_fcu_default(
        &self,
        current_head: BlockHash,
        new_head: BlockHash,
        timestamp: Option<u64>,
    ) -> AuthResult<ForkchoiceUpdated> {
        let op_params = if self.is_op() {
            // insert OP block info tx
            let txs = vec![get_op_block_info_tx()];

            // Parse the last 8 bytes of extra_data as a FixedBytes<8>
            if self.genesis_block.extra_data().len() < 9 {
                return Err(AuthProviderError::ExtraDataTooShort);
            }
            let params = FixedBytes::<8>::from_slice(&self.genesis_block.extra_data()[1..]);
            Some(OpPayloadParams {
                transactions: Some(txs),
                gas_limit: self.genesis_block.gas_limit().into(),
                eip_1559_params: Some(params),
            })
        } else {
            None
        };

        self.call_forkchoice_updated(
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
        .map_err(AuthProviderError::from)
    }

    /// Calls the correct `engine_newPayload` method depending on the given [`ExecutionPayload`] and its
    /// versioned variant. Returns the [`EngineApiMessageVersion`] depending on the payload's version.
    ///
    /// # Panics
    /// If the given payload is a V3 payload, but a parent beacon block root is provided as `None`.
    pub async fn call_new_payload(
        &self,
        payload: ExecutionPayload,
        parent_beacon_block_root: Option<B256>,
        versioned_hashes: Vec<B256>,
        execution_requests: Option<RequestsOrHash>,
    ) -> AuthResult<EngineApiMessageVersion> {
        match payload {
            ExecutionPayload::V3(payload) => {
                match self.message_version {
                    EngineApiMessageVersion::V3 => {
                        // We expect the caller
                        let parent_beacon_block_root =
                            parent_beacon_block_root.ok_or(AuthProviderError::InvalidPayload(
                                self.message_version,
                                Some("parent_beacon_block_root is required for V3 payloads"),
                            ))?;
                        self.inner
                            .new_payload_v3_wait(
                                payload,
                                versioned_hashes,
                                parent_beacon_block_root,
                            )
                            .await?;

                        Ok(EngineApiMessageVersion::V3)
                    }
                    EngineApiMessageVersion::V4 => {
                        // We expect the caller
                        let parent_beacon_block_root =
                            parent_beacon_block_root.ok_or(AuthProviderError::InvalidPayload(
                                self.message_version,
                                Some("parent_beacon_block_root is required for V4 payloads"),
                            ))?;
                        // ... and executionRequests
                        let execution_requests =
                            execution_requests.ok_or(AuthProviderError::InvalidPayload(
                                self.message_version,
                                Some("execution_requests is required for V4 payloads"),
                            ))?;
                        self.inner
                            .new_payload_v4_wait(
                                payload,
                                versioned_hashes,
                                parent_beacon_block_root,
                                execution_requests,
                            )
                            .await?;
                        Ok(EngineApiMessageVersion::V4)
                    }
                    _ => panic!("invalid message type for V3 payload"),
                }
            }
            ExecutionPayload::V2(payload) => {
                let input = ExecutionPayloadInputV2 {
                    execution_payload: payload.payload_inner,
                    withdrawals: Some(payload.withdrawals),
                };

                self.inner.new_payload_v2_wait(input).await?;

                Ok(EngineApiMessageVersion::V2)
            }
            ExecutionPayload::V1(payload) => {
                self.inner.new_payload_v1_wait(payload).await?;

                Ok(EngineApiMessageVersion::V1)
            }
        }
    }
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
            min_base_fee: None, // TODO: support min_base_fee
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

#[async_trait]
impl<N: Network + NetworkAttributes> AdvanceChain for AuthProvider<N> {
    /// Advance the chain by calling `engine_forkchoiceUpdated` (FCU) and `engine_newPayload` methods.
    async fn advance_chain(&self, block_time_secs: u64) -> AuthResult<()> {
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
        let rq = header.requests_hash().unwrap_or_default();

        macro_rules! call_new_payload {
            ($payload:expr) => {
                self.call_new_payload(
                    $payload.to_owned().into(),
                    Some(B256::ZERO),
                    vec![],
                    Some(rq.into()),
                )
                .await?;
                info!("new payload sent.");
            };
        }

        macro_rules! call_fcu {
            ($block:expr, $payload_block_hash:expr) => {
                //
                // second FCU: call with updated block head from new payload
                //
                let res = self
                    .call_fcu_default($block.header().hash(), $payload_block_hash, None)
                    .await?;
                debug!("FCU call sent. Payload ID: {:?}", res.payload_id);
            };
        }

        //
        // call getPayload with new payload ID, then
        // call the appropriate newPayload method with the fresh payload, then
        // call forkchoice_updated with the payload's block hash
        //
        match self.message_version {
            EngineApiMessageVersion::V1 => {
                let payload = engine_client.get_payload_v1(payload_id).await?;
                call_new_payload!(payload);
                call_fcu!(block, payload.block_hash);
            }
            EngineApiMessageVersion::V2 => {
                let payload = engine_client
                    .get_payload_v2(payload_id)
                    .await?
                    .execution_payload;
                call_new_payload!(payload);
                call_fcu!(block, payload.into_payload().block_hash());
            }
            EngineApiMessageVersion::V3 => {
                let payload = engine_client
                    .get_payload_v3(payload_id)
                    .await?
                    .execution_payload;
                call_new_payload!(payload);
                call_fcu!(block, payload.payload_inner.payload_inner.block_hash);
            }
            EngineApiMessageVersion::V4 => {
                let payload = engine_client.get_payload_v4(payload_id).await?;
                call_new_payload!(payload.execution_payload);
                call_fcu!(
                    block,
                    payload
                        .execution_payload
                        .payload_inner
                        .payload_inner
                        .block_hash
                );
            }
            EngineApiMessageVersion::V5 => todo!("V5 is not yet supported"),
        };

        Ok(())
    }
}

impl GetBlockTxs for AuthProvider<Ethereum> {
    type Block = alloy::rpc::types::eth::Block;

    fn get_block_txs(&self, block: &Self::Block) -> Vec<impl TxLike> {
        block
            .transactions()
            .clone()
            .into_transactions()
            .map(AnyTx::Eth)
            .collect()
    }
}

impl GetBlockTxs for AuthProvider<Optimism> {
    type Block = <op_alloy_network::Optimism as Network>::BlockResponse;

    fn get_block_txs(&self, block: &Self::Block) -> Vec<impl TxLike> {
        block
            .transactions
            .clone()
            .into_transactions()
            .map(AnyTx::Op)
            .collect::<Vec<_>>()
    }
}

macro_rules! impl_block_to_payload_for_network {
    ($net:path) => {
        #[async_trait]
        impl BlockToPayload for AuthProvider<$net> {
            async fn block_to_payload(&self, block_num: u64) -> AuthResult<ExecutionPayload> {
                let blk = self
                    .inner
                    .get_block(block_num.into())
                    .full()
                    .await?
                    .expect("block");

                // 2) encode txs to raw rlp as required by payloads
                let txs = self.get_block_txs(&blk);
                let txs: Vec<Bytes> = txs.into_iter().map(|t| t.encoded_2718()).collect();

                // 3) choose payload version by era:
                //    - pre-Shanghai: V1
                //    - Shanghai+: V2 (withdrawals)
                //    - Cancun/4844: V3 (blob fields) or V4 (adds parentBeaconBlockRoot)
                let header = blk.header().to_owned();

                let payload_v1 = ExecutionPayloadV1 {
                    block_hash: header.hash(),
                    parent_hash: header.parent_hash(),
                    fee_recipient: header.beneficiary(),
                    state_root: header.state_root(),
                    receipts_root: header.receipts_root(),
                    logs_bloom: header.logs_bloom(),
                    prev_randao: header.mix_hash().unwrap_or_default(),
                    block_number: header.number(),
                    gas_limit: header.gas_limit(),
                    gas_used: header.gas_used(),
                    timestamp: header.timestamp(),
                    extra_data: header.extra_data().to_owned(),
                    base_fee_per_gas: header
                        .base_fee_per_gas()
                        .map(U256::from)
                        .unwrap_or_default(),
                    transactions: txs,
                };
                let payload_v2 = ExecutionPayloadV2 {
                    withdrawals: blk.withdrawals.unwrap_or_default().0,
                    payload_inner: payload_v1,
                };

                let payload: ExecutionPayload =
                    if let (Some(blob_gas_used), Some(excess_blob_gas)) =
                        (header.blob_gas_used(), header.excess_blob_gas())
                    {
                        ExecutionPayloadV3 {
                            payload_inner: payload_v2,
                            blob_gas_used,
                            excess_blob_gas,
                        }
                        .into()
                    } else {
                        // Shanghai+ (withdrawals) → V2; pre-Shanghai → V1
                        payload_v2.into()
                    };

                Ok(payload)
            }
        }
    };
}

macro_rules! impl_replay_chain_for_network {
    ($net:path) => {
        #[async_trait]
        impl ReplayChain for AuthProvider<$net> {
            async fn replay_chain_segment(
                &self,
                start_block: u64,
                end_block: Option<u64>,
            ) -> AuthResult<ChainReplayResults> {
                let engine_client = &self.inner;

                // check block range
                let blocknum_head = end_block.unwrap_or(engine_client.get_block_number().await?);
                if start_block >= blocknum_head {
                    return Err(AuthProviderError::InvalidBlockRange(
                        start_block,
                        blocknum_head,
                    ));
                }
                if start_block < 1 {
                    return Err(AuthProviderError::InvalidBlockStart(start_block));
                }

                info!("replaying blocks: {start_block} - {blocknum_head}...");

                let get_block = async |blocknum: u64| {
                    engine_client
                        .get_block(blocknum.into())
                        .full()
                        .await?
                        .ok_or(AuthProviderError::MissingBlock(blocknum_head))
                };

                // get all blocks first, so we only account for the execution speed
                // (time spent processing after we've retrieved all the blocks)
                let mut blocks = vec![];
                // start at parent of start_block to get parent hash
                for i in start_block - 1..blocknum_head {
                    let blk = get_block(i).await?;
                    blocks.push(blk);
                }

                let mut gas_used = 0u128;
                let start_timestamp = Instant::now();

                // windows will allow us to read the first block while using the second as the block to execute
                for blockwin in blocks.windows(2) {
                    let prev_block = &blockwin[0];
                    let current_block = &blockwin[1];
                    let prev_hash = prev_block.header.hash();
                    gas_used += current_block.header.gas_used as u128;

                    /*** update chain head w/ FCU ***/
                    self.call_fcu_default(prev_hash, current_block.header.hash(), None)
                        .await?;

                    /*** recreate block payload ***/
                    let payload = self.block_to_payload(current_block.header.number).await?;

                    let sidecars = None; // TODO: do we need to support sidecars?
                    let txs = self.get_block_txs(&current_block);
                    let versioned_hashes = derive_blob_versioned_hashes(&txs, sidecars)?;

                    let requests = match current_block.header.requests_hash {
                        Some(h) => RequestsOrHash::Hash(h),
                        None => RequestsOrHash::Hash(B256::ZERO), // “no requests” hash; acceptable for pre-7685 eras
                    };

                    self.call_new_payload(
                        payload,
                        current_block.header.parent_beacon_block_root,
                        versioned_hashes,
                        Some(requests),
                    )
                    .await?;
                }
                let time_elapsed = Instant::now().duration_since(start_timestamp);

                Ok(ChainReplayResults {
                    gas_used,
                    time_elapsed,
                })
            }
        }
    };
}

impl<N: Network + NetworkAttributes> RpcId for AuthProvider<N> {
    fn genesis_hash(&self) -> FixedBytes<32> {
        self.genesis_block.hash()
    }

    fn rpc_url(&self) -> String {
        self.rpc_url.clone()
    }
}

impl_block_to_payload_for_network!(Ethereum);
impl_block_to_payload_for_network!(Optimism);
impl_replay_chain_for_network!(Ethereum);
impl_replay_chain_for_network!(Optimism);
impl ControlChain for AuthProvider<Optimism> {}
impl ControlChain for AuthProvider<Ethereum> {}

impl NetworkAttributes for Optimism {
    type PayloadAttributes = OpPayloadAttributes;
    fn is_op() -> bool {
        true
    }
}

impl NetworkAttributes for Ethereum {
    type PayloadAttributes = PayloadAttributes;
    fn is_op() -> bool {
        false
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

/// EIP-4844: versioned_hash = 0x01 || sha256(commitment)[1..32]
#[inline]
fn kzg_to_versioned_hash(commitment: &[u8; 48]) -> B256 {
    let mut out = [0u8; 32];
    out[0] = 0x01;
    let d = Sha256::digest(commitment);
    out[1..32].copy_from_slice(&d[1..32]);
    B256::from(out)
}

/// Return all blob versioned hashes for all 4844 txs in `blk`.
/// Prefers vhashes embedded in the tx; falls back to sidecar commitments.
pub fn derive_blob_versioned_hashes(
    txs: &[impl TxLike],
    sidecars: Option<&HashMap<B256, BlobTransactionSidecar>>,
) -> AuthResult<Vec<B256>> {
    let mut out = Vec::new();

    for tx in txs {
        if let Some(vhs) = tx.blob_versioned_hashes() {
            out.extend_from_slice(vhs);
            continue;
        }

        if let Some(sc) = sidecars.and_then(|m| m.get(&tx.tx_hash())) {
            for c in &sc.commitments {
                let c48: FixedBytes<48> = *c;
                out.push(kzg_to_versioned_hash(c48.as_slice().try_into().unwrap()));
            }
        }
    }
    Ok(out)
}

fn get_op_block_info_tx() -> Bytes {
    let deposit_tx = TxDeposit {
        source_hash: B256::default(),
        from: address!("DeaDDEaDDeAdDeAdDEAdDEaddeAddEAdDEAd0001"),
        to: TxKind::Call(address!("4200000000000000000000000000000000000015")),
        mint: 0,
        value: U256::default(),
        gas_limit: 210000,
        is_system_transaction: false,
        input: FJORD_DATA.into(),
    };

    // Create a temporary signer for the deposit
    let signer = Signer::random();

    // sign tx
    let signed_tx = signer
        .sign_tx(OpTypedTransaction::Deposit(deposit_tx))
        .expect("failed to sign tx");

    // convert tx to bytes
    signed_tx.encoded_2718().into()
}
