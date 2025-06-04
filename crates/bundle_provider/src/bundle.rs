use alloy::rpc::types::mev::{EthBundleHash, EthSendBundle};
use tracing::debug;

use crate::{
    error::BundleProviderError,
    revert_bundle::{BundlesFromRequest, RevertProtectBundleRequest},
    BundleClient,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum BundleType {
    #[default]
    L1,
    RevertProtected,
}

#[derive(Clone, Debug)]
pub enum TypedBundle {
    L1(EthSendBundle),
    RevertProtected(RevertProtectBundleRequest),
}

impl TypedBundle {
    pub async fn send(&self, client: &BundleClient) -> Result<(), BundleProviderError> {
        match self {
            TypedBundle::L1(b) => {
                let res = client.send_bundle::<_, EthBundleHash>(b).await?;
                debug!("{:?} bundle sent, response: {:?}", b, res);
            }
            TypedBundle::RevertProtected(b) => {
                // make a RevertProtectBundle from each tx in the bundle
                // and send it to the client
                for bundle in b.to_bundles() {
                    let res = client.send_bundle::<_, EthBundleHash>(&bundle).await?;
                    debug!(
                        "{:?} Revert protected bundle sent, response: {:?}",
                        bundle, res
                    );
                }
            }
        }
        Ok(())
    }
}
