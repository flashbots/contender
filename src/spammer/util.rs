#[cfg(test)]
pub mod test {
    use std::{collections::HashMap, sync::Arc};

    use alloy::providers::PendingTransactionConfig;
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
            _req: NamedTxRequest,
            _extra: Option<HashMap<String, String>>,
            _tx_handler: Option<Arc<TxActorHandle>>,
        ) -> Option<JoinHandle<()>> {
            println!("MockCallback::on_tx_sent: tx_hash={}", _tx_res.tx_hash());
            None
        }
    }
}
