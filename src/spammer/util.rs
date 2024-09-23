#[cfg(test)]
pub mod test {
    use std::collections::HashMap;

    use alloy::primitives::TxHash;
    use tokio::task::JoinHandle;

    use crate::{generator::NamedTxRequest, spammer::OnTxSent};

    pub struct MockCallback;
    impl OnTxSent<String> for MockCallback {
        fn on_tx_sent(
            &self,
            _tx_hash: TxHash,
            _req: NamedTxRequest,
            _extra: Option<HashMap<String, String>>,
        ) -> Option<JoinHandle<()>> {
            println!("MockCallback::on_tx_sent: tx_hash={}", _tx_hash);
            None
        }
    }
}
