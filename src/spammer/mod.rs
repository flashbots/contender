use crate::{generator::Generator, Result};
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

pub struct Spammer<G>
where
    G: Generator,
{
    generator: G,
    rpc_client: Arc<RootProvider<Http<Client>>>,
}

impl<G> Spammer<G>
where
    G: Generator,
{
    pub fn new(generator: G, rpc_url: impl AsRef<str>) -> Self {
        let rpc_client =
            ProviderBuilder::new().on_http(Url::parse(rpc_url.as_ref()).expect("Invalid RPC URL"));
        Self {
            generator,
            rpc_client: Arc::new(rpc_client),
        }
    }

    /// Send transactions to the RPC at a given rate. Actual rate may vary; this is only the attempted sending rate.
    pub fn spam_rpc(&self, tx_per_second: usize, duration: usize) -> Result<()> {
        let tx_requests = self.generator.get_spam_txs(tx_per_second * duration)?;
        let interval = std::time::Duration::from_millis(1_000 / tx_per_second as u64);

        for tx in tx_requests {
            // clone Arc
            let rpc_client = self.rpc_client.to_owned();

            // send tx to the RPC asynchrononsly
            spawn_task(async move {
                // TODO: sign tx if priv_key is provided
                println!("sending tx: {:?}", tx);
                let res = rpc_client.send_transaction(tx).await.unwrap();
                let receipt = res.get_receipt().await.unwrap();
                println!("receipt: {:?}", receipt);
            });

            // sleep for interval
            std::thread::sleep(interval);
        }

        Ok(())
    }
}
