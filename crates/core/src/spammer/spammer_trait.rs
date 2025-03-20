use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::Stream;
use futures::StreamExt;

use crate::generator::named_txs::ExecutionRequest;
use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, templater::Templater, types::AnyProvider, Generator, PlanConfig},
    test_scenario::TestScenario,
    Result,
};

use super::tx_callback::OnBatchSent;
use super::SpamTrigger;
use super::{tx_actor::TxActorHandle, OnTxSent};

async fn flush_tx_cache<
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
>(
    block_start: u64,
    run_id: Option<u64>,
    scenario: &TestScenario<D, S, P>,
) -> Result<()> {
    let mut block_counter = 0;
    if let Some(run_id) = run_id {
        loop {
            let cache_size = scenario
                .msg_handle
                .flush_cache(run_id, block_start + block_counter as u64)
                .await
                .map_err(|e| {
                    ContenderError::SpamError("failed to flush cache", e.to_string().into())
                })?;
            if cache_size == 0 {
                break;
            }

            block_counter += 1;
        }
        println!("done. run_id={}", run_id);
    }
    Ok(())
}

async fn execute_spammer<
    F: OnTxSent + OnBatchSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
>(
    cursor: &mut futures::stream::Take<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>,
    scenario: &mut TestScenario<D, S, P>,
    tx_req_chunks: &[Vec<ExecutionRequest>],
    callback: Arc<F>, // contains callbacks called by each tx after it's sent, and after each batch is sent
) -> Result<()> {
    let mut tick = 0;
    while let Some(trigger) = cursor.next().await {
        let trigger = trigger.to_owned();
        let payloads = scenario.prepare_spam(&tx_req_chunks[tick]).await?;
        let spam_tasks = scenario
            .execute_spam(trigger, &payloads, callback.clone())
            .await?;
        println!("[{}] executing {} spam tasks", tick, spam_tasks.len());
        for task in spam_tasks {
            let res = task.await;
            if let Err(e) = res {
                eprintln!("spam task failed: {:?}", e);
            }
        }
        callback.on_batch_sent();
        tick += 1;
    }

    Ok(())
}

async fn get_spam_tx_chunks<
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
>(
    scenario: &TestScenario<D, S, P>,
    txs_per_period: usize,
    num_periods: usize,
) -> Result<Vec<Vec<ExecutionRequest>>> {
    let tx_requests = scenario
        .load_txs(crate::generator::PlanType::Spam(
            txs_per_period * num_periods,
            |_named_req| Ok(None), // we can look at the named request here if needed
        ))
        .await?;
    Ok(tx_requests
        .chunks(txs_per_period)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<Vec<ExecutionRequest>>>())
}

pub trait Spammer<F, D, S, P>
where
    F: OnTxSent + OnBatchSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    fn get_msg_handler(&self, db: Arc<D>, rpc_client: Arc<AnyProvider>) -> TxActorHandle {
        TxActorHandle::new(12, db.clone(), rpc_client.clone())
    }

    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>;

    fn spam_rpc(
        &self,
        scenario: &mut TestScenario<D, S, P>,
        txs_per_period: usize,
        num_periods: usize,
        run_id: Option<u64>,
        callback: Arc<F>,
    ) -> impl std::future::Future<Output = Result<()>> {
        async move {
            let tx_req_chunks = get_spam_tx_chunks(scenario, txs_per_period, num_periods).await?;
            let start_block = scenario
                .rpc_client
                .get_block_number()
                .await
                .map_err(|e| ContenderError::with_err(e, "failed to get block number"))?;
            let mut cursor = self.on_spam(scenario).await?.take(num_periods);

            // run spammer within tokio::select! to allow for graceful shutdown
            let spam_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\nCTRL-C received, stopping spamming...");
                    false
                },
                _ = execute_spammer(&mut cursor, scenario, &tx_req_chunks, callback) => {
                    true
                }
            };
            if !spam_finished {
                println!("Spammer terminated. Press CTRL-C again to stop result collection...");
            }

            let flush_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\nCTRL-C received, stopping result collection...");
                    let _ = scenario.msg_handle.stop().await;
                    false
                },
                _ = flush_tx_cache(start_block, run_id, scenario) => {
                    true
                }
            };
            if !flush_finished {
                let runid = if let Some(run_id) = run_id {
                    format!(" run_id={run_id}")
                } else {
                    "".to_string()
                };
                println!("Result collection terminated. Some results may not have been saved to the database.{runid}");
            }

            Ok(())
        }
    }
}
