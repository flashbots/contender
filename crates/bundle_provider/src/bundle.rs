use alloy::rpc::types::mev::EthSendBundle;

use crate::{
    revert_bundle::{RevertProtectBundle, RevertProtectBundleRequest},
    BundleClient,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum BundleType {
    #[default]
    L1,
    Revertable,
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
