use crate::{
    db::database::DbOps,
    generator::{
        seeder::Seeder,
        types::{PlanType, RpcProvider},
        Generator,
    },
    spammer::OnTxSent,
    test_scenario::TestScenario,
    Result,
};
use alloy::hex::ToHexExt;
use alloy::providers::Provider;
use alloy::providers::ProviderBuilder;
use std::sync::Arc;
use tokio::task;

pub struct TimedSpammer<F, D, S>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
{
    scenario: TestScenario<D, S>,
    rpc_client: Arc<RpcProvider>,
    callback_handler: Arc<F>,
}

impl<F, D, S> TimedSpammer<F, D, S>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
{
    pub fn new(scenario: TestScenario<D, S>, callback_handler: F) -> Self {
        let rpc_client = ProviderBuilder::new().on_http(scenario.rpc_url.to_owned());
        Self {
            scenario,
            rpc_client: Arc::new(rpc_client),
            callback_handler: Arc::new(callback_handler),
        }
    }

    /// Send transactions to the RPC at a given rate. Actual rate may vary; this is only the attempted sending rate.
    pub async fn spam_rpc(&self, tx_per_second: usize, duration: usize) -> Result<()> {
        let tx_requests = self
            .scenario
            .load_txs(PlanType::Spam(tx_per_second * duration, |_| Ok(None)))
            .await?;
        let interval = std::time::Duration::from_nanos(1_000_000_000 / tx_per_second as u64);
        let mut tasks = vec![];

        for tx in tx_requests {
            // clone Arcs
            let rpc_client = self.rpc_client.clone();
            let callback_handler = self.callback_handler.clone();

            // send tx to the RPC asynchrononsly
            tasks.push(task::spawn(async move {
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
