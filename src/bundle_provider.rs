use alloy::network::EthereumWallet;
use alloy::primitives::{Bytes, B256};
use alloy::signers::local::PrivateKeySigner;
use alloy::transports::http::reqwest::Url;
use jsonrpsee::{core::client::ClientT, rpc_params};
use serde::{Deserialize, Serialize};

pub struct BundleClient {
    url: Url,
    _wallet: EthereumWallet, // TODO: use to sign payload for auth header
}

impl BundleClient {
    pub fn new(url: String, auth_signer: PrivateKeySigner) -> Self {
        let wallet = EthereumWallet::new(auth_signer);
        Self {
            url: url.parse().expect("invalid bundle RPC URL"),
            _wallet: wallet,
        }
    }

    pub async fn send_bundle(&self, bundle: EthSendBundle) -> Result<(), String> {
        // TODO: make this more efficient
        let client = jsonrpsee::http_client::HttpClient::builder()
            // .set_headers(HeaderMap::from_iter(vec![(
            //     HeaderName::from_str("X-Flashbots-Signature").unwrap(),
            //     HeaderValue::from_str("test").unwrap(),
            // )]))
            .build(self.url.clone())
            .expect("failed to connect to RPC provider");

        let res: Result<String, _> = client.request("eth_sendBundle", rpc_params![bundle]).await;
        println!("sent bundle {:?}", res);

        Ok(())
    }
}

// testing:
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EthSendBundle {
    /// A list of hex-encoded signed transactions
    pub txs: Vec<Bytes>,
    /// hex-encoded block number for which this bundle is valid
    #[serde(with = "alloy_serde::quantity")]
    pub block_number: u64,
    /// unix timestamp when this bundle becomes active
    #[serde(
        default,
        with = "alloy_serde::quantity::opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_timestamp: Option<u64>,
    /// unix timestamp how long this bundle stays valid
    #[serde(
        default,
        with = "alloy_serde::quantity::opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_timestamp: Option<u64>,
    /// list of hashes of possibly reverting txs
    #[serde(
        default
        // this doesn't work on rbuilder:
        // , skip_serializing_if = "Vec::is_empty"
    )]
    pub reverting_tx_hashes: Vec<B256>,
    /// UUID that can be used to cancel/replace this bundle
    #[serde(
        default,
        rename = "replacementUuid",
        skip_serializing_if = "Option::is_none"
    )]
    pub replacement_uuid: Option<String>,
}
