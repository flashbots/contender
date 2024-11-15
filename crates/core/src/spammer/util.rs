#[cfg(test)]
pub mod test {
    use std::{collections::HashMap, str::FromStr, sync::Arc};

    use alloy::{providers::PendingTransactionConfig, signers::local::PrivateKeySigner};
    use tokio::task::JoinHandle;

    use crate::{
        generator::NamedTxRequest,
        spammer::{tx_actor::TxActorHandle, OnTxSent},
    };

    pub struct MockCallback;
    impl OnTxSent<String> for MockCallback {
        fn on_tx_sent(
            &self,
            _tx_res: PendingTransactionConfig,
            _req: &NamedTxRequest,
            _extra: Option<HashMap<String, String>>,
            _tx_handler: Option<Arc<TxActorHandle>>,
        ) -> Option<JoinHandle<()>> {
            println!("MockCallback::on_tx_sent: tx_hash={}", _tx_res.tx_hash());
            None
        }
    }

    pub fn get_test_signers() -> Vec<PrivateKeySigner> {
        vec![
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        ]
        .iter()
        .map(|s| PrivateKeySigner::from_str(s).unwrap())
        .collect::<Vec<PrivateKeySigner>>()
    }
}
