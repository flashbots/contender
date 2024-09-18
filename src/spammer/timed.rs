use super::SpamCallback;
use crate::{generator::Generator, Result};
use alloy::hex::ToHexExt;
use alloy::{
    providers::Provider,
    transports::http::{reqwest::Url, Http},
};
use alloy::{
    providers::{ProviderBuilder, RootProvider},
    transports::http::Client,
};
use std::sync::Arc;
use tokio::task::spawn as spawn_task;

pub struct TimedSpammer<G, F>
where
    G: Generator,
    F: SpamCallback + Send + Sync + 'static,
{
    generator: G,
    rpc_client: Arc<RootProvider<Http<Client>>>,
    callback_handler: Arc<F>,
}

impl<G, F> TimedSpammer<G, F>
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

    /// Send transactions to the RPC at a given rate. Actual rate may vary; this is only the attempted sending rate.
    pub async fn spam_rpc(&self, tx_per_second: usize, duration: usize) -> Result<()> {
        let tx_requests = self.generator.get_txs(tx_per_second * duration)?;
        let interval = std::time::Duration::from_nanos(1_000_000_000 / tx_per_second as u64);
        let mut tasks = vec![];

        for tx in tx_requests {
            // clone Arcs
            let rpc_client = self.rpc_client.clone();
            let callback_handler = self.callback_handler.clone();

            // send tx to the RPC asynchrononsly
            tasks.push(spawn_task(async move {
                let tx_req = &tx.tx;
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
                let res = rpc_client.send_transaction(tx.tx).await.unwrap();
                let maybe_handle = callback_handler.on_tx_sent(*res.tx_hash(), tx.name);
                if let Some(handle) = maybe_handle {
                    handle.await.unwrap();
                } // ignore None values so we don't attempt to await them
            }));

            // sleep for interval
            std::thread::sleep(interval);
        }

        // join on all handles
        for task in tasks {
            task.await.map_err(|e| {
                crate::error::ContenderError::SpamError(
                    "failed to join task handle",
                    Some(e.to_string()),
                )
            })?;
        }

        Ok(())
    }
}
