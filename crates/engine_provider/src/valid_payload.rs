//! This is an extension trait for any provider that implements the engine API, to wait for a VALID
//! response. This is useful for benchmarking, as it allows us to wait for a payload to be valid
//! before sending additional calls.

use crate::{auth_provider::NetworkAttributes, engine::EngineApi};
use alloy::providers::Network;
use alloy::transports::TransportResult;
use alloy::{eips::eip7685::Requests, primitives::B256};
use alloy_rpc_types_engine::{
    ExecutionPayloadInputV2, ExecutionPayloadV1, ExecutionPayloadV3, ForkchoiceState,
    ForkchoiceUpdated, PayloadStatus,
};
use tracing::error;

/// An extension trait for providers that implement the engine API, to wait for a VALID response.
#[async_trait::async_trait]
pub trait EngineApiValidWaitExt<N: NetworkAttributes>: Send + Sync {
    /// Calls `engine_newPayloadV1` with the given [ExecutionPayloadV1], and waits until the
    /// response is VALID.
    async fn new_payload_v1_wait(
        &self,
        payload: ExecutionPayloadV1,
    ) -> TransportResult<PayloadStatus>;

    /// Calls `engine_newPayloadV2` with the given [ExecutionPayloadInputV2], and waits until the
    /// response is VALID.
    async fn new_payload_v2_wait(
        &self,
        payload: ExecutionPayloadInputV2,
    ) -> TransportResult<PayloadStatus>;

    /// Calls `engine_newPayloadV3` with the given [ExecutionPayloadV3], parent beacon block root,
    /// and versioned hashes, and waits until the response is VALID.
    async fn new_payload_v3_wait(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> TransportResult<PayloadStatus>;

    /// Calls `engine_newPayloadV4` with the given [ExecutionPayloadV3], parent beacon block root,
    /// versioned hashes, and execution requests, and waits until the response is VALID.
    async fn new_payload_v4_wait(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Requests,
    ) -> TransportResult<PayloadStatus>;

    /// Calls `engine_forkChoiceUpdatedV1` with the given [ForkchoiceState] and optional
    /// [PayloadAttributes], and waits until the response is VALID.
    async fn fork_choice_updated_v1_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;

    /// Calls `engine_forkChoiceUpdatedV2` with the given [ForkchoiceState] and optional
    /// [PayloadAttributes], and waits until the response is VALID.
    async fn fork_choice_updated_v2_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;

    /// Calls `engine_forkChoiceUpdatedV3` with the given [ForkchoiceState] and optional
    /// [PayloadAttributes], and waits until the response is VALID.
    async fn fork_choice_updated_v3_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated>;
}

#[async_trait::async_trait]
impl<N, P> EngineApiValidWaitExt<N> for P
where
    N: Network + NetworkAttributes,
    P: EngineApi<N>,
{
    async fn new_payload_v1_wait(
        &self,
        payload: ExecutionPayloadV1,
    ) -> TransportResult<PayloadStatus> {
        let mut status = self.new_payload_v1(payload.clone()).await?;
        while !status.is_valid() {
            if status.is_invalid() {
                error!(?status, ?payload, "Invalid newPayloadV1",);
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "Invalid newPayloadV1",
                ));
            }
            status = self.new_payload_v1(payload.clone()).await?;
        }
        Ok(status)
    }

    async fn new_payload_v2_wait(
        &self,
        payload: ExecutionPayloadInputV2,
    ) -> TransportResult<PayloadStatus> {
        let mut status = self.new_payload_v2(payload.clone()).await?;
        while !status.is_valid() {
            if status.is_invalid() {
                error!(?status, ?payload, "Invalid newPayloadV2",);
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "Invalid newPayloadV2",
                ));
            }
            status = self.new_payload_v2(payload.clone()).await?;
        }
        Ok(status)
    }

    async fn new_payload_v3_wait(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> TransportResult<PayloadStatus> {
        let mut status = self
            .new_payload_v3(
                payload.clone(),
                versioned_hashes.clone(),
                parent_beacon_block_root,
            )
            .await?;
        while !status.is_valid() {
            if status.is_invalid() {
                error!(
                    ?status,
                    ?payload,
                    ?versioned_hashes,
                    ?parent_beacon_block_root,
                    "Invalid newPayloadV3",
                );
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "Invalid newPayloadV3",
                ));
            }
            if status.is_syncing() {
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "invalid range: no canonical state found for parent of requested block",
                ));
            }
            status = self
                .new_payload_v3(
                    payload.clone(),
                    versioned_hashes.clone(),
                    parent_beacon_block_root,
                )
                .await?;
        }
        Ok(status)
    }

    async fn new_payload_v4_wait(
        &self,
        payload: ExecutionPayloadV3,
        versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Requests,
    ) -> TransportResult<PayloadStatus> {
        let mut status = self
            .new_payload_v4(
                payload.clone(),
                versioned_hashes.clone(),
                parent_beacon_block_root,
                execution_requests.clone(),
            )
            .await?;

        while !status.is_valid() {
            if status.is_invalid() {
                error!(
                    ?status,
                    ?payload,
                    ?versioned_hashes,
                    ?parent_beacon_block_root,
                    ?execution_requests,
                    "Invalid newPayloadV4",
                );
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "Invalid newPayloadV4",
                ));
            }
            if status.is_syncing() {
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "invalid range: no canonical state found for parent of requested block",
                ));
            }
            status = self
                .new_payload_v4(
                    payload.clone(),
                    versioned_hashes.clone(),
                    parent_beacon_block_root,
                    execution_requests.clone(),
                )
                .await?;
        }
        Ok(status)
    }

    async fn fork_choice_updated_v1_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        let mut status = self
            .fork_choice_updated_v1(fork_choice_state, payload_attributes.clone())
            .await?;

        while !status.is_valid() {
            if status.is_invalid() {
                error!(
                    ?status,
                    ?fork_choice_state,
                    ?payload_attributes,
                    "Invalid forkchoiceUpdatedV1 message",
                );
                panic!("Invalid forkchoiceUpdatedV1: {status:?}");
            }
            if status.is_syncing() {
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "invalid range: no canonical state found for parent of requested block",
                ));
            }
            status = self
                .fork_choice_updated_v1(fork_choice_state, payload_attributes.clone())
                .await?;
        }

        Ok(status)
    }

    async fn fork_choice_updated_v2_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        let mut status = self
            .fork_choice_updated_v2(fork_choice_state, payload_attributes.clone())
            .await?;

        while !status.is_valid() {
            if status.is_invalid() {
                error!(
                    ?status,
                    ?fork_choice_state,
                    ?payload_attributes,
                    "Invalid forkchoiceUpdatedV2 message",
                );
                panic!("Invalid forkchoiceUpdatedV2: {status:?}");
            }
            if status.is_syncing() {
                return Err(alloy_json_rpc::RpcError::UnsupportedFeature(
                    "invalid range: no canonical state found for parent of requested block",
                ));
            }
            status = self
                .fork_choice_updated_v2(fork_choice_state, payload_attributes.clone())
                .await?;
        }

        Ok(status)
    }

    async fn fork_choice_updated_v3_wait(
        &self,
        fork_choice_state: ForkchoiceState,
        payload_attributes: Option<N::PayloadAttributes>,
    ) -> TransportResult<ForkchoiceUpdated> {
        let mut status = self
            .fork_choice_updated_v3(fork_choice_state, payload_attributes.clone())
            .await?;

        while !status.is_valid() {
            if status.is_invalid() {
                error!(
                    ?status,
                    ?fork_choice_state,
                    ?payload_attributes,
                    "Invalid forkchoiceUpdatedV3 message",
                );
                panic!("Invalid forkchoiceUpdatedV3: {status:?}");
            }
            status = self
                .fork_choice_updated_v3(fork_choice_state, payload_attributes.clone())
                .await?;
        }

        Ok(status)
    }
}
