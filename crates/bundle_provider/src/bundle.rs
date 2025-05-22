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
    Revertable,
}

#[derive(Clone, Debug)]
pub enum Bundle {
    L1(EthSendBundle),
    Revertable(RevertProtectBundleRequest),
}

impl Bundle {
    pub async fn send(&self, client: &BundleClient) -> Result<(), BundleProviderError> {
        match self {
            Bundle::L1(b) => client.send_bundle::<_, EthBundleHash>(b).await,
            Bundle::Revertable(b) => {
                // make a RevertProtectBundle from each tx in the bundle
                // and send it to the client
                for tx in b.txs.to_owned() {
                    let req = RevertProtectBundleRequest::new().with_txs(vec![tx]);
                    client
                        .send_bundle::<_, String>(RevertProtectBundle::from(req))
                        .await?;
                }
                Ok(())
            }
        }
    }
}
