use crate::db::database::DbOps;
use crate::error::ContenderError;
use crate::generator::seeder::Seeder;
use crate::generator::types::{PlanType, RpcProvider};
use crate::generator::Generator;
use crate::scenario::test_scenario::TestScenario;
use crate::Result;
use alloy::hex::ToHexExt;
use alloy::network::TransactionBuilder;
use alloy::primitives::FixedBytes;
use alloy::{
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task;

use super::OnTxSent;

pub struct BlockwiseSpammer<F, D, S>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    F: OnTxSent + Send + Sync + 'static,
{
    scenario: TestScenario<D, S>,
    rpc_client: Arc<RpcProvider>,
    callback_handler: Arc<F>,
}

impl<F, D, S> BlockwiseSpammer<F, D, S>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
{
    pub fn new(
        scenario: TestScenario<D, S>,
        callback_handler: F,
        rpc_url: impl AsRef<str>,
    ) -> Self {
        let rpc_client =
            ProviderBuilder::new().on_http(Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL"));

        Self {
            scenario,
            rpc_client: Arc::new(rpc_client),
            callback_handler: Arc::new(callback_handler),
        }
    }

    pub async fn spam_rpc(
        &self,
        txs_per_block: usize,
        num_blocks: usize,
        run_id: Option<usize>,
    ) -> Result<()> {
        // generate tx requests
        let tx_requests = self
            .scenario
            .load_txs(PlanType::Spam(txs_per_block * num_blocks, |_| Ok(None)))
            .await?;
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
            for (addr, _) in self.scenario.wallet_map.iter() {
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
                    .scenario
                    .wallet_map
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
                    let maybe_handle = callback_handler
                        .on_tx_sent(*res.tx_hash(), run_id.map(|id| id.to_string()));
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
    use crate::{
        generator::{testfile::tests::get_test_signers, util::test::spawn_anvil},
        spammer::util::test::MockCallback,
    };

    use super::*;

    #[tokio::test]
    async fn watches_blocks_and_spams_them() {
        let anvil = spawn_anvil();
        println!("anvil url: {}", anvil.endpoint_url());
        let conf = crate::generator::testfile::tests::get_composite_testconfig();
        let db = crate::db::sqlite::SqliteDb::new_memory();
        let seed = crate::generator::RandSeed::from_str("444444444444");
        let scenario = TestScenario::new(conf, db, anvil.endpoint_url(), seed, &get_test_signers());
        let callback_handler = MockCallback;
        let spammer =
            BlockwiseSpammer::new(scenario, callback_handler, anvil.endpoint_url().to_string());

        let result = spammer.spam_rpc(10, 3, None).await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
