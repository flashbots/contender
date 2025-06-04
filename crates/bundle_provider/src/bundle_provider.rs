use alloy::{
    network::AnyNetwork,
    primitives::Bytes,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::{
        json_rpc::{RpcRecv, RpcSend},
        types::mev::EthSendBundle,
    },
    transports::http::reqwest::IntoUrl,
};

use crate::{bundle::TypedBundle, error::BundleProviderError};

/// A helper wrapper around a RPC client that can be used to call `eth_sendBundle`.
#[derive(Debug)]
pub struct BundleClient {
    client: RootProvider<AnyNetwork>,
}

impl BundleClient {
    /// Creates a new [`BundleClient`] with the given URL.
    pub fn new(url: impl IntoUrl) -> Result<Self, BundleProviderError> {
        let provider = ProviderBuilder::default().connect_http(
            url.into_url()
                .map_err(|_| BundleProviderError::InvalidUrl)?,
        );
        Ok(Self { client: provider })
    }

    /// Sends a bundle using `eth_sendBundle`, discarding the response.
    pub async fn send_bundle<Bundle: RpcSend, Response: RpcRecv>(
        &self,
        bundle: Bundle,
    ) -> Result<Option<Response>, BundleProviderError> {
        // Result contents optional because some endpoints don't return this response
        self.client
            .raw_request::<_, Option<Response>>("eth_sendBundle".into(), [bundle])
            .await
            .map_err(|e| BundleProviderError::SendBundleError(e.into()))
    }
}

/// Creates a new bundle with the given transactions and block number, setting the rest of the
/// fields to default values.
#[inline]
pub fn new_basic_bundle(txs: Vec<Bytes>, block_number: u64) -> TypedBundle {
    TypedBundle::L1(EthSendBundle {
        txs,
        block_number,
        ..Default::default()
    })
}
