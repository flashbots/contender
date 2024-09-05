use crate::{
    generator::{rand_seed::RandSeed, Generator},
    Result,
};
use alloy::{
    primitives::FixedBytes,
    providers::Provider,
    transports::http::{reqwest::Url, Http},
};
use alloy::{
    providers::{ProviderBuilder, RootProvider},
    transports::http::Client,
};
use tokio::task::spawn as spawn_task;

pub struct Spammer<G: Generator> {
    generator: G,
    rpc_client: Box<RootProvider<Http<Client>>>,
    seed: RandSeed,
    // TODO: add signer for priv_key tx signatures
}

impl<G> Spammer<G>
where
    G: Generator,
{
    pub fn new(
        generator: G,
        rpc_url: String,
        seed: Option<RandSeed>,
        priv_key: Option<FixedBytes<32>>,
    ) -> Self {
        let seed = seed.unwrap_or_default();
        let rpc_client =
            ProviderBuilder::new().on_http(Url::parse(&rpc_url).expect("Invalid RPC URL"));
        Self {
            generator,
            rpc_client: Box::new(rpc_client),
            seed,
        }
    }

    /// Send transactions to the RPC at a given rate. Actual rate may vary; this is only the attempted sending rate.
    pub fn spam_rpc(&self, tx_per_second: usize, duration: usize) -> Result<()> {
        let tx_requests = self
            .generator
            .get_spam_txs(tx_per_second * duration, Some(self.seed.to_owned()))?;
        let interval = std::time::Duration::from_millis(1_000 / tx_per_second as u64);

        for tx in tx_requests {
            // dev note: probably losing some efficiency cloning the client and tx for every request -- is there a better way?
            let rpc_client = self.rpc_client.to_owned();
            let tx = tx.to_owned();
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
