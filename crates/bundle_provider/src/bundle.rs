use alloy::rpc::types::mev::{EthBundleHash, EthSendBundle};

use crate::{
    error::BundleProviderError,
    revert_bundle::{RevertProtectBundle, RevertProtectBundleRequest},
    BundleClient,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum BundleType {
    #[default]
    L1,
    RevertProtected,
}

#[derive(Clone, Debug)]
pub enum Bundle {
    L1(EthSendBundle),
    RevertProtected(RevertProtectBundleRequest),
}

impl Bundle {
    pub async fn send(&self, client: &BundleClient) -> Result<(), BundleProviderError> {
        match self {
            Bundle::L1(b) => client.send_bundle::<_, EthBundleHash>(b).await,
            Bundle::RevertProtected(b) => {
                // make a RevertProtectBundle from each tx in the bundle
                // and send it to the client
                for tx in &b.txs {
                    let req = RevertProtectBundleRequest::new().with_txs(vec![tx.to_owned()]);
                    client
                        .send_bundle::<_, EthBundleHash>(RevertProtectBundle::from(req))
                        .await?;
                }
                Ok(())
            }
        }
    }
}
