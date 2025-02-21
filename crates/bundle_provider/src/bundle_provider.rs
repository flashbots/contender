use alloy::{
    network::AnyNetwork,
    primitives::Bytes,
    providers::{Provider, ProviderBuilder, RootProvider},
    rpc::types::mev::{EthBundleHash, EthSendBundle},
    transports::http::reqwest::IntoUrl,
};

/// A helper wrapper around a RPC client that can be used to call `eth_sendBundle`.
#[derive(Debug)]
pub struct BundleClient {
    client: RootProvider<AnyNetwork>,
}

impl BundleClient {
    /// Creates a new [`BundleClient`] with the given URL.
    pub fn new(url: impl IntoUrl) -> Self {
        let provider = ProviderBuilder::default().on_http(url.into_url().unwrap());
        Self { client: provider }
    }

    /// Sends a bundle using `eth_sendBundle`, discarding the response.
    pub async fn send_bundle(&self, bundle: EthSendBundle) -> Result<(), String> {
        // Result contents optional because some endpoints don't return this response
        self.client
            .raw_request::<EthSendBundle, Option<EthBundleHash>>("eth_sendBundle".into(), bundle)
            .await
            .map_err(|e| format!("Failed to send bundle: {:?}", e))?;

        Ok(())
    }
}

/// Creates a new bundle with the given transactions and block number, setting the rest of the
/// fields to default values.
#[inline]
pub fn new_basic_bundle(txs: Vec<Bytes>, block_number: u64) -> EthSendBundle {
    EthSendBundle {
        txs,
        block_number,
        ..Default::default()
    }
}
