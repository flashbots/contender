#[cfg(test)]
pub mod test {
    use std::{collections::HashMap, str::FromStr, sync::Arc};

    use alloy::{
        consensus::{constants::GWEI_TO_WEI, TxType},
        network::{EthereumWallet, TransactionBuilder},
        primitives::{Address, U256},
        providers::{PendingTransactionConfig, Provider},
        rpc::types::TransactionRequest,
        signers::local::PrivateKeySigner,
    };
    use tokio::task::JoinHandle;

    use crate::{
        generator::{types::EthProvider, util::complete_tx_request, NamedTxRequest},
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
        [
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        ]
        .iter()
        .map(|s| PrivateKeySigner::from_str(s).unwrap())
        .collect::<Vec<PrivateKeySigner>>()
    }

    pub async fn fund_account(
        sender: &PrivateKeySigner,
        recipient: Address,
        amount: U256,
        rpc_client: &EthProvider,
        nonce: Option<u64>,
        tx_type: TxType,
    ) -> Result<PendingTransactionConfig, Box<dyn std::error::Error>> {
        println!(
            "funding account {} with user account {}",
            recipient,
            sender.address()
        );

        let gas_price = rpc_client.get_gas_price().await?;
        let nonce = nonce.unwrap_or(rpc_client.get_transaction_count(sender.address()).await?);
        let chain_id = rpc_client.get_chain_id().await?;
        let mut tx_req = TransactionRequest {
            from: Some(sender.address()),
            to: Some(alloy::primitives::TxKind::Call(recipient)),
            value: Some(amount),
            nonce: Some(nonce),
            ..Default::default()
        };

        complete_tx_request(&mut tx_req, tx_type, gas_price, 0 as u128, 21000, chain_id);

        let eth_wallet = EthereumWallet::from(sender.to_owned());
        let tx = tx_req.build(&eth_wallet).await?;
        let res = rpc_client.send_tx_envelope(tx).await?;

        Ok(res.into_inner())
    }
}
