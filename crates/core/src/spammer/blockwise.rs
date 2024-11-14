use crate::bundle_provider::{BundleClient, EthSendBundle};
use crate::db::DbOps;
use crate::error::ContenderError;
use crate::generator::named_txs::ExecutionRequest;
use crate::generator::seeder::Seeder;
use crate::generator::templater::Templater;
use crate::generator::types::{AnyProvider, EthProvider, PlanType};
use crate::generator::{Generator, PlanConfig};
use crate::spammer::ExecutionPayload;
use crate::test_scenario::TestScenario;
use crate::Result;
use alloy::consensus::Transaction;
use alloy::eips::eip2718::Encodable2718;
use alloy::hex::ToHexExt;
use alloy::network::{AnyNetwork, EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, FixedBytes};
use alloy::providers::{PendingTransactionConfig, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use futures::StreamExt;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use tokio::task;

use super::tx_actor::TxActorHandle;
use super::OnTxSent;

/// Defines the number of blocks to target with a single bundle.
const BUNDLE_BLOCK_TOLERANCE: usize = 5;

pub struct BlockwiseSpammer<F, D, S, P>
where
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    F: OnTxSent + Send + Sync + 'static,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    scenario: TestScenario<D, S, P>,
    msg_handler: Arc<TxActorHandle>,
    rpc_client: AnyProvider,
    eth_client: Arc<EthProvider>,
    bundle_client: Option<Arc<BundleClient>>,
    callback_handler: Arc<F>,
    nonces: HashMap<Address, u64>,
    gas_limits: HashMap<FixedBytes<4>, u128>,
}

impl<F, D, S, P> BlockwiseSpammer<F, D, S, P>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    pub async fn new(scenario: TestScenario<D, S, P>, callback_handler: F) -> Self {
        let rpc_client = ProviderBuilder::new()
            .network::<AnyNetwork>()
            .on_http(scenario.rpc_url.to_owned());
        let eth_client = Arc::new(ProviderBuilder::new().on_http(scenario.rpc_url.to_owned()));
        let bundle_client = scenario
            .builder_rpc_url
            .to_owned()
            .map(|url| Arc::new(BundleClient::new(url.into())));
        let msg_handler = Arc::new(TxActorHandle::new(
            12,
            scenario.db.clone(),
            Arc::new(rpc_client.to_owned()),
        ));
        let callback_handler = Arc::new(callback_handler);

        // get nonce for each signer and put it into a hashmap
        let mut nonces = HashMap::new();
        for (addr, _) in scenario.wallet_map.iter() {
            let nonce = eth_client
                .get_transaction_count(*addr)
                .await
                .expect("failed to retrieve nonce");
            nonces.insert(*addr, nonce);
        }

        // track gas limits for each function signature
        let gas_limits = HashMap::<FixedBytes<4>, u128>::new();

        Self {
            scenario,
            rpc_client,
            eth_client,
            bundle_client,
            callback_handler,
            msg_handler,
            nonces,
            gas_limits,
        }
    }

    async fn prepare_tx_req(
        &mut self,
        tx_req: &TransactionRequest,
        gas_price: u128,
        chain_id: u64,
    ) -> Result<(TransactionRequest, EthereumWallet)> {
        let from = tx_req.from.expect("missing from address");
        let nonce = self
            .nonces
            .get(&from)
            .expect("failed to get nonce")
            .to_owned();
        /*
            Increment nonce assuming the tx will succeed.
            Note: if any tx fails, txs with higher nonces will also fail.
            However, we'll get a fresh nonce next block.
        */
        self.nonces.insert(from.to_owned(), nonce + 1);
        let fn_sig = FixedBytes::<4>::from_slice(
            tx_req
                .input
                .input
                .to_owned()
                .map(|b| b.split_at(4).0.to_owned())
                .expect("invalid function call")
                .as_slice(),
        );
        if !self.gas_limits.contains_key(fn_sig.as_slice()) {
            let gas_limit = self
                .eth_client
                .estimate_gas(&tx_req.to_owned())
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to estimate gas"))?;
            self.gas_limits.insert(fn_sig, gas_limit);
        }
        // query hashmaps for gaslimit & signer of this tx
        let gas_limit = self
            .gas_limits
            .get(&fn_sig)
            .expect("failed to get gas limit")
            .to_owned();
        let signer = self
            .scenario
            .wallet_map
            .get(&from)
            .expect("failed to create signer")
            .to_owned();

        let full_tx = tx_req
            .clone()
            .with_nonce(nonce)
            .with_max_fee_per_gas(gas_price + (gas_price / 5))
            .with_max_priority_fee_per_gas(gas_price)
            .with_chain_id(chain_id)
            .with_gas_limit(gas_limit + (gas_limit / 4));

        Ok((full_tx, signer))
    }

    pub async fn spam_rpc(
        &mut self,
        txs_per_block: usize,
        num_blocks: usize,
        run_id: Option<u64>,
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
        let mut last_block_number = 0;

        // get chain id before we start spamming
        let chain_id = self
            .rpc_client
            .get_chain_id()
            .await
            .map_err(|e| ContenderError::with_err(e, "failed to get chain id"))?;

        // init block stream
        let poller = self
            .rpc_client
            .watch_blocks()
            .await
            .map_err(|e| ContenderError::with_err(e, "failed to create block poller"))?;
        let mut stream = poller
            .into_stream()
            .flat_map(futures::stream::iter)
            .take(num_blocks);

        let mut tasks = vec![];

        while let Some(block_hash) = stream.next().await {
            let block_txs = tx_req_chunks[block_offset].clone();
            block_offset += 1;

            let block = self
                .rpc_client
                .get_block_by_hash(block_hash, alloy::rpc::types::BlockTransactionsKind::Hashes)
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get block"))?
                .ok_or(ContenderError::SpamError("no block found", None))?;
            last_block_number = block.header.number;

            // get gas price
            let gas_price = self
                .rpc_client
                .get_gas_price()
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get gas price"))?;

            for (idx, tx) in block_txs.into_iter().enumerate() {
                let gas_price = gas_price + (idx as u128 * 1e9 as u128);
                // clone muh Arcs
                let eth_client = self.eth_client.clone();
                let callback_handler = self.callback_handler.clone();
                let tx_handler = self.msg_handler.clone();

                // prepare tx/bundle with nonce, gas price, signatures, etc
                let payload = match tx {
                    ExecutionRequest::Bundle(reqs) => {
                        if self.bundle_client.is_none() {
                            return Err(ContenderError::SpamError(
                                "Bundle client not found. Add the `--builder-url` flag to send bundles.",
                                None,
                            ));
                        }

                        // prepare each tx in the bundle (increment nonce, set gas price, etc)
                        let mut bundle_txs = vec![];
                        for req in reqs.iter() {
                            let tx_req = req.tx.to_owned();
                            let (tx_req, signer) = self
                                .prepare_tx_req(&tx_req, gas_price, chain_id)
                                .await
                                .map_err(|e| ContenderError::with_err(e, "failed to prepare tx"))?;

                            // sign tx
                            let tx_envelope = tx_req.build(&signer).await.map_err(|e| {
                                ContenderError::with_err(e, "bad request: failed to build tx")
                            })?;

                            bundle_txs.push(tx_envelope);
                        }
                        ExecutionPayload::SignedTxBundle(bundle_txs, reqs)
                    }
                    ExecutionRequest::Tx(req) => {
                        let tx_req = req.tx.to_owned();

                        let (tx_req, signer) = self
                            .prepare_tx_req(&tx_req, gas_price, chain_id)
                            .await
                            .map_err(|e| ContenderError::with_err(e, "failed to prepare tx"))?;

                        // sign tx
                        let tx_envelope = tx_req.to_owned().build(&signer).await.map_err(|e| {
                            ContenderError::with_err(e, "bad request: failed to build tx")
                        })?;

                        println!(
                            "sending tx {} from={} to={:?} input={} value={}",
                            tx_envelope.tx_hash(),
                            tx_req.from.map(|s| s.encode_hex()).unwrap_or_default(),
                            tx_envelope.to().to(),
                            tx_req
                                .input
                                .input
                                .as_ref()
                                .map(|s| s.encode_hex())
                                .unwrap_or_default(),
                            tx_req
                                .value
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "0".to_owned())
                        );

                        ExecutionPayload::SignedTx(tx_envelope, req)
                    }
                };

                let bundle_client = self.bundle_client.clone();

                // build, sign, and send tx/bundle in a new task (green thread)
                tasks.push(task::spawn(async move {
                    let provider = ProviderBuilder::new().on_provider(eth_client);
                    let start_timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("failed to get timestamp")
                        .as_millis() as usize;

                    let mut extra = HashMap::new();
                    extra.insert("start_timestamp".to_owned(), start_timestamp.to_string());

                    // triggers & awaits callback for every individual tx (including txs in a bundle)
                    let handles = match payload {
                        ExecutionPayload::SignedTx(signed_tx, req) => {
                            let res = provider
                                .send_tx_envelope(signed_tx)
                                .await
                                .expect("RPC error: failed to send tx");
                            let maybe_handle = callback_handler.on_tx_sent(
                                res.into_inner(),
                                &req.to_owned(),
                                extra.clone().into(),
                                Some(tx_handler),
                            );
                            vec![maybe_handle]
                        }
                        ExecutionPayload::SignedTxBundle(signed_txs, reqs) => {
                            let mut bundle_txs = vec![];
                            for tx in &signed_txs {
                                let mut raw_tx = vec![];
                                tx.encode_2718(&mut raw_tx);
                                bundle_txs.push(raw_tx);
                            }
                            let rpc_bundle = EthSendBundle {
                                txs: bundle_txs.into_iter().map(|tx| tx.into()).collect(),
                                block_number: last_block_number,
                                min_timestamp: None,
                                max_timestamp: None,
                                reverting_tx_hashes: vec![],
                                replacement_uuid: None,
                            };
                            if let Some(bundle_client) = bundle_client {
                                println!("spamming bundle: {:?}", rpc_bundle);
                                // send `num_blocks` bundles at a time, targeting each successive block
                                for i in 1..(num_blocks + BUNDLE_BLOCK_TOLERANCE) {
                                    let mut rpc_bundle = rpc_bundle.clone();
                                    rpc_bundle.block_number = last_block_number + i as u64;
                                    let res = rpc_bundle.send_to_builder(&bundle_client).await;
                                    if let Err(e) = res {
                                        eprintln!("failed to send bundle: {:?}", e);
                                    }
                                }
                            } else {
                                panic!("no bundle client provided. Please add the `--builder-url` flag");
                            }

                            let mut tx_handles = vec![];
                            for (tx, req) in signed_txs.into_iter().zip(&reqs) {
                                let maybe_handle = callback_handler.on_tx_sent(
                                    PendingTransactionConfig::new(tx.tx_hash().to_owned()),
                                    &req,
                                    extra.clone().into(),
                                    Some(tx_handler.clone()),
                                );
                                tx_handles.push(maybe_handle);
                            }
                            tx_handles
                        }
                    };
                    for handle in handles {
                        if let Some(handle) = handle {
                            handle.await.expect("callback task failed");
                        }
                    }
                }));
            }

            println!("new block: {block_hash}");
            if let Some(run_id) = run_id {
                // write this block's txs to DB
                let _ = self
                    .msg_handler
                    .flush_cache(run_id, last_block_number)
                    .await
                    .map_err(|e| ContenderError::with_err(e.deref(), "failed to flush cache"))?;
            }
        }

        for task in tasks {
            let _ = task.await;
        }

        // wait until there are no txs left in the cache, or until we time out
        let mut timeout_counter = 0;
        if let Some(run_id) = run_id {
            loop {
                timeout_counter += 1;
                if timeout_counter > 12 {
                    println!("Quitting due to timeout.");
                    break;
                }
                let cache_size = self
                    .msg_handler
                    .flush_cache(run_id, last_block_number)
                    .await
                    .map_err(|e| ContenderError::with_err(e.deref(), "failed to empty cache"))?;
                if cache_size == 0 {
                    break;
                }
                last_block_number += 1;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        db::MockDb,
        generator::util::test::spawn_anvil,
        spammer::util::test::{get_test_signers, MockCallback},
        test_scenario::tests::MockConfig,
    };

    use super::*;

    #[tokio::test]
    async fn watches_blocks_and_spams_them() {
        let anvil = spawn_anvil();
        println!("anvil url: {}", anvil.endpoint_url());
        let seed = crate::generator::RandSeed::from_str("444444444444");
        let scenario = TestScenario::new(
            MockConfig,
            MockDb.into(),
            anvil.endpoint_url(),
            None,
            seed,
            get_test_signers().as_slice(),
        );
        let callback_handler = MockCallback;
        let mut spammer = BlockwiseSpammer::new(scenario, callback_handler).await;

        let result = spammer.spam_rpc(10, 3, None).await;
        println!("{:?}", result);
        assert!(result.is_ok());
    }
}
