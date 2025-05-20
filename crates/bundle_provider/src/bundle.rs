use alloy::rpc::types::mev::EthSendBundle;

use crate::{
    revert_bundle::{RevertProtectBundle, RevertProtectBundleRequest},
    BundleClient,
};

#[derive(Clone, Copy, Debug)]
pub enum BundleType {
    L1,
    Revertable,
}

impl Default for BundleType {
    fn default() -> Self {
        BundleType::L1
    }
}

#[derive(Clone, Debug)]
pub enum Bundle {
    L1(EthSendBundle),
    Revertable(RevertProtectBundleRequest),
}

impl Bundle {
    pub async fn send(&self, client: &BundleClient) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Bundle::L1(b) => client.send_bundle(b).await,
            Bundle::Revertable(b) => client.send_bundle(RevertProtectBundle::from(b)).await,
        }
    }
}
