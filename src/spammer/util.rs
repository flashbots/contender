#[cfg(test)]
pub mod test {
    use alloy::primitives::TxHash;
    use tokio::task::JoinHandle;

    use crate::spammer::OnTxSent;

    pub struct MockCallback;
    impl OnTxSent for MockCallback {
        fn on_tx_sent(&self, _tx_hash: TxHash, _name: Option<String>) -> Option<JoinHandle<()>> {
            println!("MockCallback::on_tx_sent: tx_hash={}", _tx_hash);
            None
        }
    }
}
