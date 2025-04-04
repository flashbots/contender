use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::Stream;
use futures::StreamExt;

use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, templater::Templater, types::AnyProvider, PlanConfig},
    test_scenario::TestScenario,
    Result,
};

use super::SpamTrigger;
use super::{tx_actor::TxActorHandle, OnTxSent};

pub trait Spammer<F, D, S, P>
where
    F: OnTxSent + Send + Sync + 'static,
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
        sent_tx_callback: Arc<F>,
    ) -> impl std::future::Future<Output = Result<()>> {
        async move {
            let tx_req_chunks = scenario
                .get_spam_tx_chunks(txs_per_period, num_periods)
                .await?;
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
                _ = scenario.execute_spammer(&mut cursor, &tx_req_chunks, sent_tx_callback) => {
                    true
                }
            };
            if !spam_finished {
                println!("Spammer terminated. Press CTRL-C again to stop result collection...");
            }

            // collect results from cached pending txs
            let flush_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\nCTRL-C received, stopping result collection...");
                    let _ = scenario.msg_handle.stop().await;
                    false
                },
                _ = scenario.flush_tx_cache(start_block, run_id) => {
                    true
                }
            };
            if !flush_finished {
                println!("Result collection terminated. Some pending txs may not have been saved to the database.");
                println!("Saving unconfirmed txs to DB. Press CTRL-C again to stop...");
            }

            // clear out unconfirmed txs from the cache
            let dump_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\nCTRL-C received, stopping tx cache dump...");
                    false
                },
                _ = scenario.dump_tx_cache(run_id) => {
                    true
                }
            };
            if !dump_finished {
                println!("Tx cache dump terminated. Some unconfirmed txs may not have been saved to the database.");
            }
            let run_id = run_id
                .map(|id| format!("run_id: {}", id))
                .unwrap_or_default();
            println!("done. {run_id}");
            Ok(())
        }
    }
}
