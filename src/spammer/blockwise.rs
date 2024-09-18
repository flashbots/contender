use crate::{generator::Generator, Result};
use alloy::hex::ToHexExt;
use alloy::{
    providers::{Provider, ProviderBuilder},
    transports::http::reqwest::Url,
};
use futures::StreamExt;
use std::sync::Arc;

use super::{util::RpcProvider, SpamCallback};

pub struct BlockwiseSpammer<G, F>
where
    G: Generator,
    F: SpamCallback + Send + Sync + 'static,
{
    generator: G,
    rpc_client: Arc<RpcProvider>,
    callback_handler: Arc<F>,
}

impl<G, F> BlockwiseSpammer<G, F>
where
    G: Generator,
    F: SpamCallback + Send + Sync + 'static,
{
    pub fn new(generator: G, callback_handler: F, rpc_url: impl AsRef<str>) -> Self {
        let rpc_client =
            ProviderBuilder::new().on_http(Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL"));
        Self {
            generator,
            rpc_client: Arc::new(rpc_client),
            callback_handler: Arc::new(callback_handler),
        }
    }

    pub async fn spam_rpc(&self, txs_per_block: usize, num_blocks: usize) -> Result<()> {
        // generate tx requests
        let tx_requests = self.generator.get_txs(txs_per_block * num_blocks)?;
        let tx_req_chunks = tx_requests.chunks(txs_per_block);
        let tx_req_chunks: Vec<_> = tx_req_chunks.map(|slice| slice.to_vec()).collect();
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

        while let Some(block_hash) = stream.next().await {
            let block_txs = tx_req_chunks[block_offset].clone();
            block_offset += 1;

            for tx in block_txs {
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

                // clone muh Arcs
                let rpc_client = self.rpc_client.clone();
                let callback_handler = self.callback_handler.clone();

                tasks.push(tokio::task::spawn(async move {
                    let res = rpc_client.send_transaction(tx_req).await.unwrap();
                    let maybe_handle = callback_handler.on_tx_sent(*res.tx_hash(), tx.name.clone());
                    if let Some(handle) = maybe_handle {
                        handle.await.expect("callback task failed");
                    } // ignore None values so we don't attempt to await them
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
        let spammer = BlockwiseSpammer::new(generator, callback_handler, rpc_url);

        let result = spammer.spam_rpc(10, 10).await;
        assert!(result.is_ok());
    }
}
