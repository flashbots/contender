use crate::error::ContenderError;
use crate::{generator::Generator, Result};
use alloy::hex::ToHexExt;
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, FixedBytes};
use alloy::signers::local::PrivateKeySigner;
use alloy::{
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::task;

use super::{util::RpcProvider, SpamCallback};

pub struct BlockwiseSpammer<G, F>
where
    G: Generator,
    F: SpamCallback + Send + Sync + 'static,
{
    generator: G,
    rpc_client: Arc<RpcProvider>,
    callback_handler: Arc<F>,
    signers: HashMap<Address, EthereumWallet>,
}

impl<G, F> BlockwiseSpammer<G, F>
where
    G: Generator,
    F: SpamCallback + Send + Sync + 'static,
{
    pub fn new(
        generator: G,
        callback_handler: F,
        rpc_url: impl AsRef<str>,
        prv_keys: &[impl AsRef<str>],
    ) -> Self {
        let rpc_client =
            ProviderBuilder::new().on_http(Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL"));

        let signers = prv_keys.iter().map(|k| {
            let key = k.as_ref();
            let signer = PrivateKeySigner::from_str(key).expect("Invalid private key");
            let addr = signer.address();
            (addr, signer)
        });
        // populate hashmap with signers where address is the key, signer is the value
        let mut signer_map: HashMap<Address, EthereumWallet> = HashMap::new();
        for (addr, signer) in signers {
            signer_map.insert(addr, signer.into());
        }

        Self {
            generator,
            rpc_client: Arc::new(rpc_client),
            callback_handler: Arc::new(callback_handler),
            signers: signer_map,
        }
    }

    pub async fn spam_rpc(&self, txs_per_block: usize, num_blocks: usize) -> Result<()> {
        // generate tx requests
        let tx_requests = self.generator.get_txs(txs_per_block * num_blocks)?;
        let tx_req_chunks: Vec<_> = tx_requests
            .chunks(txs_per_block)
            .map(|slice| slice.to_vec())
            .collect();
        let mut block_offset = 0;

        // init block stream
        let poller = self.rpc_client.watch_blocks().await.map_err(|e| {
            crate::error::ContenderError::SpamError(
                "failed to create block poller",
                e.to_string().into(),
            )
        })?;
        let mut stream = poller
            .into_stream()
            .flat_map(futures::stream::iter)
            .take(num_blocks);

        let mut tasks = vec![];
        let mut gas_limits = HashMap::<FixedBytes<4>, u128>::new();

        while let Some(block_hash) = stream.next().await {
            let block_txs = tx_req_chunks[block_offset].clone();
            block_offset += 1;

            // get gas price
            let gas_price = self.rpc_client.get_gas_price().await.map_err(|e| {
                ContenderError::SpamError("failed to get gas price", e.to_string().into())
            })?;
            // get nonce for each signer and put it into a hashmap
            let mut nonces = HashMap::new();
            for (addr, _signer) in self.signers.iter() {
                let nonce = self
                    .rpc_client
                    .get_transaction_count(*addr)
                    .await
                    .map_err(|_| ContenderError::SpamError("failed to get nonce", None))?;
                nonces.insert(*addr, nonce);
            }

            for (idx, tx) in block_txs.into_iter().enumerate() {
                let gas_price = gas_price + (idx as u128 * 1e9 as u128);
                let tx_req = tx.tx.to_owned();
                println!(
                    "sending tx. from={} to={} input={}",
                    tx_req.from.map(|s| s.encode_hex()).unwrap_or_default(),
                    tx_req
                        .to
                        .map(|s| s.to().map(|s| *s))
                        .flatten()
                        .map(|s| s.encode_hex())
                        .unwrap_or_default(),
                    tx_req
                        .input
                        .input
                        .as_ref()
                        .map(|s| s.encode_hex())
                        .unwrap_or_default(),
                );

                let from = &tx_req.from.expect("missing from address");
                let nonce = nonces.get(from).expect("failed to get nonce").to_owned();

                let fn_sig = FixedBytes::<4>::from_slice(
                    tx.tx
                        .to_owned()
                        .input
                        .input
                        .map(|b| b.split_at(4).0.to_owned())
                        .expect("invalid function call")
                        .as_slice(),
                );

                if !gas_limits.contains_key(fn_sig.as_slice()) {
                    let gas_limit = self
                        .rpc_client
                        .estimate_gas(&tx.tx.to_owned())
                        .await
                        .map_err(|e| {
                            ContenderError::SpamError(
                                "failed to estimate gas",
                                e.to_string().into(),
                            )
                        })?;
                    gas_limits.insert(fn_sig, gas_limit);
                }

                // clone muh Arcs
                let rpc_client = self.rpc_client.clone();
                let callback_handler = self.callback_handler.clone();

                // query hashmaps for gaslimit & signer of this tx
                let gas_limit = gas_limits
                    .get(&fn_sig)
                    .expect("failed to get gas limit")
                    .to_owned();
                let signer = self
                    .signers
                    .get(from)
                    .expect("failed to create signer")
                    .to_owned();

                // optimistically update nonce since we've succeeded so far
                nonces.insert(from.to_owned(), nonce + 1);

                // build, sign, and send tx in a new task (green thread)
                tasks.push(task::spawn(async move {
                    let provider = ProviderBuilder::new()
                        .wallet(signer)
                        .on_provider(rpc_client);

                    let full_tx = tx_req
                        .clone()
                        .with_nonce(nonce)
                        .with_gas_price(gas_price)
                        .with_gas_limit(gas_limit);

                    let res = provider.send_transaction(full_tx).await.unwrap();
                    let maybe_handle = callback_handler.on_tx_sent(*res.tx_hash(), tx.name.clone());
                    if let Some(handle) = maybe_handle {
                        handle.await.expect("callback task failed");
                    }
                    // ignore None values so we don't attempt to await them
                }));
            }
            println!("new block: {block_hash}");
        }

        for task in tasks {
            let _ = task.await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::spammer::util::test::MockCallback;
    // use alloy::

    use super::*;

    #[tokio::test]
    #[ignore = "reason: requires a running RPC node on localhost:8545"]
    async fn watch_blocks() {
        let conf = crate::generator::testfile::tests::get_composite_testconfig();
        let db = crate::db::sqlite::SqliteDb::new_memory();
        let seed = crate::generator::RandSeed::from_str("444444444444");
        let generator = crate::generator::testfile::SpamGenerator::new(conf, &seed, db.clone());
        let callback_handler = MockCallback;
        let rpc_url = "http://localhost:8545";
        let spammer = BlockwiseSpammer::new(
            generator,
            callback_handler,
            rpc_url,
            &vec![
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
                "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
                "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
            ],
        );

        let result = spammer.spam_rpc(10, 3).await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
