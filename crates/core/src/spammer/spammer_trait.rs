use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::Stream;
use futures::StreamExt;

use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, templater::Templater, types::AnyProvider, Generator, PlanConfig},
    test_scenario::TestScenario,
    Result,
};

use super::SpamTrigger;
use super::{tx_actor::TxActorHandle, OnTxSent};

pub trait Spammer<F, D, S, P>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    fn sent_tx_callback(&self) -> Arc<F>;

    fn get_msg_handler(&self, db: Arc<D>, rpc_client: Arc<AnyProvider>) -> TxActorHandle {
        TxActorHandle::new(12, db.clone(), rpc_client.clone())
    }

    fn msg_handler(&self) -> Arc<TxActorHandle>;

    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>;

    fn spam_rpc(
        &self,
        scenario: &mut TestScenario<D, S, P>,
        txs_per_period: usize,
        num_periods: usize,
        run_id: Option<u64>,
    ) -> impl std::future::Future<Output = Result<()>> {
        async move {
            let tx_requests = scenario
                .load_txs(crate::generator::PlanType::Spam(
                    txs_per_period * num_periods,
                    |_named_req| Ok(None), // we can look at the named request here if needed
                ))
                .await?;
            let tx_req_chunks = tx_requests.chunks(txs_per_period).collect::<Vec<&[_]>>();
            let block_num = scenario
                .rpc_client
                .get_block_number()
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get block number"))?;

            let mut tick = 0;
            let mut cursor = self.on_spam(scenario).await.take(num_periods);

            while let Some(trigger) = cursor.next().await {
                let trigger = trigger.to_owned();
                let payloads = scenario.prepare_spam(tx_req_chunks[tick]).await?;
                let spam_tasks = scenario
                    .execute_spam(
                        trigger,
                        &payloads,
                        self.sent_tx_callback(),
                        self.msg_handler(),
                    )
                    .await?;
                for task in spam_tasks {
                    task.await
                        .map_err(|e| ContenderError::with_err(e, "spam task failed"))?;
                }
                tick += 1;
            }

            let mut tock = 0;
            let mut timeout_counter = 0;
            if let Some(run_id) = run_id {
                loop {
                    timeout_counter += 1;
                    if timeout_counter > 5 {
                        println!("quitting due to timeout");
                        break;
                    }
                    let cache_size = self
                        .msg_handler()
                        .flush_cache(run_id, block_num + tock as u64)
                        .await
                        .expect("failed to flush cache");
                    if cache_size == 0 {
                        break;
                    }
                    tock += 1;
                }
            }

            Ok(())
        }
    }
}
