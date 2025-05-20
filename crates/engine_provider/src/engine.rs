use std::str::FromStr;

use alloy::{
    consensus::{
        crypto::secp256k1::public_key_to_address, transaction::Recovered, SignableTransaction,
    },
    primitives::{BlockHash, Bytes, B256, U256},
    providers::Provider,
    signers::Signature,
    transports::TransportResult,
};
use alloy_rpc_types_engine::{
    ClientVersionV1, ExecutionPayloadBodiesV1, ExecutionPayloadEnvelopeV2,
    ExecutionPayloadEnvelopeV3, ExecutionPayloadEnvelopeV4, ExecutionPayloadInputV2,
    ExecutionPayloadV1, ExecutionPayloadV3, ForkchoiceState, ForkchoiceUpdated, PayloadId,
    PayloadStatus,
};
use op_alloy_consensus::OpTypedTransaction;
use reth_optimism_primitives::OpTransactionSigned;
use secp256k1::{Message, SecretKey, SECP256K1};

use crate::auth_provider_eth::NetworkAttributes;

/// Extension trait that gives access to engine API RPC methods.
///
/// Note:
/// > The provider should use a JWT authentication layer.
#[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
pub trait EngineApi<N: NetworkAttributes>: Send + Sync {
    /// Sends the given payload to the execution layer client, as specified for the Paris fork.
    ///
    /// Caution: This should not accept the `withdrawals` field
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/paris.md#engine_newpayloadv1>
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> TransportResult<PayloadStatus>;

    /// Sends the given payload to the execution layer client, as specified for the Shanghai fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/584905270d8ad665718058060267061ecfd79ca5/src/engine/shanghai.md#engine_newpayloadv2>
    async fn new_payload_v2(
        &self,
        payload: ExecutionPayloadInputV2,
    ) -> TransportResult<PayloadStatus>;

    /// Sends the given payload to the execution layer client, as specified for the Cancun fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/main/src/engine/cancun.md#engine_newpayloadv3>
    async fn new_payload_v3(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> TransportResult<PayloadStatus>;

    /// Sends the given payload to the execution layer client, as specified for the Prague fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/03911ffc053b8b806123f1fc237184b0092a485a/src/engine/prague.md#engine_newpayloadv4>
    async fn new_payload_v4(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Vec<Bytes>,
    ) -> TransportResult<PayloadStatus>;

    /// Updates the execution layer client with the given fork choice, as specified for the Paris
    /// fork.
    ///
    /// Caution: This should not accept the `withdrawals` field in the payload attributes.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/paris.md#engine_forkchoiceupdatedv1>
    async fn fork_choice_updated_v1(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;

    /// Updates the execution layer client with the given fork choice, as specified for the Shanghai
    /// fork.
    ///
    /// Caution: This should not accept the `parentBeaconBlockRoot` field in the payload attributes.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/shanghai.md#engine_forkchoiceupdatedv2>
    async fn fork_choice_updated_v2(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;

    /// Updates the execution layer client with the given fork choice, as specified for the Cancun
    /// fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/main/src/engine/cancun.md#engine_forkchoiceupdatedv3>
    async fn fork_choice_updated_v3(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;

    /// Retrieves an execution payload from a previously started build process, as specified for the
    /// Paris fork.
    ///
    /// Caution: This should not return the `withdrawals` field
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/paris.md#engine_getpayloadv1>
    ///
    /// Note:
    /// > Provider software MAY stop the corresponding build process after serving this call.
    async fn get_payload_v1(&self, payload_id: PayloadId) -> TransportResult<ExecutionPayloadV1>;

    /// Retrieves an execution payload from a previously started build process, as specified for the
    /// Shanghai fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6709c2a795b707202e93c4f2867fa0bf2640a84f/src/engine/shanghai.md#engine_getpayloadv2>
    ///
    /// Note:
    /// > Provider software MAY stop the corresponding build process after serving this call.
    async fn get_payload_v2(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV2>;

    /// Retrieves an execution payload from a previously started build process, as specified for the
    /// Cancun fork.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/main/src/engine/cancun.md#engine_getpayloadv3>
    ///
    /// Note:
    /// > Provider software MAY stop the corresponding build process after serving this call.
    async fn get_payload_v3(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV3>;

    /// Returns the most recent version of the payload that is available in the corresponding
    /// payload build process at the time of receiving this call.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/main/src/engine/prague.md#engine_getpayloadv4>
    ///
    /// Note:
    /// > Provider software MAY stop the corresponding build process after serving this call.
    async fn get_payload_v4(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV4>;

    /// Returns the execution payload bodies by the given hash.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6452a6b194d7db269bf1dbd087a267251d3cc7f8/src/engine/shanghai.md#engine_getpayloadbodiesbyhashv1>
    async fn get_payload_bodies_by_hash_v1(
        &self,
        block_hashes: Vec<BlockHash>,
    ) -> TransportResult<ExecutionPayloadBodiesV1>;

    /// Returns the execution payload bodies by the range starting at `start`, containing `count`
    /// blocks.
    ///
    /// WARNING: This method is associated with the BeaconBlocksByRange message in the consensus
    /// layer p2p specification, meaning the input should be treated as untrusted or potentially
    /// adversarial.
    ///
    /// Implementers should take care when acting on the input to this method, specifically
    /// ensuring that the range is limited properly, and that the range boundaries are computed
    /// correctly and without panics.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6452a6b194d7db269bf1dbd087a267251d3cc7f8/src/engine/shanghai.md#engine_getpayloadbodiesbyrangev1>
    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> TransportResult<ExecutionPayloadBodiesV1>;

    /// Returns the execution client version information.
    ///
    /// Note:
    /// > The `client_version` parameter identifies the consensus client.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/main/src/engine/identification.md#engine_getclientversionv1>
    async fn get_client_version_v1(
        &self,
        client_version: ClientVersionV1,
    ) -> TransportResult<Vec<ClientVersionV1>>;

    /// Returns the list of Engine API methods supported by the execution layer client software.
    ///
    /// See also <https://github.com/ethereum/execution-apis/blob/6452a6b194d7db269bf1dbd087a267251d3cc7f8/src/engine/common.md#capabilities>
    async fn exchange_capabilities(
        &self,
        capabilities: Vec<String>,
    ) -> TransportResult<Vec<String>>;
}

#[cfg_attr(target_family = "wasm", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_family = "wasm"), async_trait::async_trait)]
impl<N, P> EngineApi<N> for P
where
    N: alloy::network::Network + NetworkAttributes,
    P: Provider<N>,
{
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> TransportResult<PayloadStatus> {
        self.client()
            .request("engine_newPayloadV1", (payload,))
            .await
    }

    async fn new_payload_v2(
        &self,
        payload: ExecutionPayloadInputV2,
    ) -> TransportResult<PayloadStatus> {
        self.client()
            .request("engine_newPayloadV2", (payload,))
            .await
    }

    async fn new_payload_v3(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> TransportResult<PayloadStatus> {
        self.client()
            .request(
                "engine_newPayloadV3",
                (payload, versioned_hashes, parent_beacon_block_root),
            )
            .await
    }

    async fn new_payload_v4(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Vec<Bytes>,
    ) -> TransportResult<PayloadStatus> {
        self.client()
            .request(
                "engine_newPayloadV4",
                (
                    payload,
                    versioned_hashes,
                    parent_beacon_block_root,
                    execution_requests,
                ),
            )
            .await
    }

    async fn fork_choice_updated_v1(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        self.client()
            .request(
                "engine_forkchoiceUpdatedV1",
                (fork_choice_state, payload_attributes),
            )
            .await
    }

    async fn fork_choice_updated_v2(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        self.client()
            .request(
                "engine_forkchoiceUpdatedV2",
                (fork_choice_state, payload_attributes),
            )
            .await
    }

    async fn fork_choice_updated_v3(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        self.client()
            .request(
                "engine_forkchoiceUpdatedV3",
                (fork_choice_state, payload_attributes),
            )
            .await
    }

    async fn get_payload_v1(&self, payload_id: PayloadId) -> TransportResult<ExecutionPayloadV1> {
        self.client()
            .request("engine_getPayloadV1", (payload_id,))
            .await
    }

    async fn get_payload_v2(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV2> {
        self.client()
            .request("engine_getPayloadV2", (payload_id,))
            .await
    }

    async fn get_payload_v3(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV3> {
        self.client()
            .request("engine_getPayloadV3", (payload_id,))
            .await
    }

    async fn get_payload_v4(
        &self,
        payload_id: PayloadId,
    ) -> TransportResult<ExecutionPayloadEnvelopeV4> {
        self.client()
            .request("engine_getPayloadV4", (payload_id,))
            .await
    }

    async fn get_payload_bodies_by_hash_v1(
        &self,
        block_hashes: Vec<BlockHash>,
    ) -> TransportResult<ExecutionPayloadBodiesV1> {
        self.client()
            .request("engine_getPayloadBodiesByHashV1", (block_hashes,))
            .await
    }

    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> TransportResult<ExecutionPayloadBodiesV1> {
        self.client()
            .request("engine_getPayloadBodiesByRangeV1", (start, count))
            .await
    }

    async fn get_client_version_v1(
        &self,
        client_version: ClientVersionV1,
    ) -> TransportResult<Vec<ClientVersionV1>> {
        self.client()
            .request("engine_getClientVersionV1", (client_version,))
            .await
    }

    async fn exchange_capabilities(
        &self,
        capabilities: Vec<String>,
    ) -> TransportResult<Vec<String>> {
        self.client()
            .request("engine_exchangeCapabilities", (capabilities,))
            .await
    }
}

/// Simple struct to sign txs/messages.
/// Mainly used to sign payout txs from the builder and to create test data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signer {
    pub address: alloy::primitives::Address,
    pub secret: SecretKey,
}

impl Signer {
    pub fn try_from_secret(secret: B256) -> Result<Self, secp256k1::Error> {
        let secret = SecretKey::from_slice(secret.as_ref())?;
        let pubkey = secret.public_key(SECP256K1);
        let address = public_key_to_address(pubkey);

        Ok(Self { address, secret })
    }

    pub fn sign_message(&self, message: B256) -> Result<Signature, secp256k1::Error> {
        let s = SECP256K1
            .sign_ecdsa_recoverable(&Message::from_digest_slice(&message[..])?, &self.secret);
        let (rec_id, data) = s.serialize_compact();

        let signature = Signature::new(
            U256::try_from_be_slice(&data[..32]).expect("The slice has at most 32 bytes"),
            U256::try_from_be_slice(&data[32..64]).expect("The slice has at most 32 bytes"),
            i32::from(rec_id) != 0,
        );
        Ok(signature)
    }

    pub fn sign_tx(
        &self,
        tx: OpTypedTransaction,
    ) -> Result<Recovered<OpTransactionSigned>, secp256k1::Error> {
        let signature_hash = match &tx {
            OpTypedTransaction::Legacy(tx) => tx.signature_hash(),
            OpTypedTransaction::Eip2930(tx) => tx.signature_hash(),
            OpTypedTransaction::Eip1559(tx) => tx.signature_hash(),
            OpTypedTransaction::Eip7702(tx) => tx.signature_hash(),
            OpTypedTransaction::Deposit(_) => B256::ZERO,
        };
        let signature = self.sign_message(signature_hash)?;
        let signed = OpTransactionSigned::new_unhashed(tx, signature);
        Ok(Recovered::new_unchecked(signed, self.address))
    }

    pub fn random() -> Self {
        Self::try_from_secret(B256::random()).expect("failed to create random signer")
    }
}

impl FromStr for Signer {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_secret(B256::from_str(s)?)
            .map_err(|e| eyre::eyre!("invalid secret key {:?}", e.to_string()))
    }
}
