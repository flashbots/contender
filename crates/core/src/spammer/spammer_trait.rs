use std::sync::atomic::AtomicBool;
use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use futures::Stream;
use futures::StreamExt;
use tracing::info;

use crate::{
    db::DbOps,
    error::ContenderError,
    generator::{seeder::Seeder, templater::Templater, types::AnyProvider, PlanConfig},
    test_scenario::TestScenario,
    Result,
};

use super::tx_callback::OnBatchSent;
use super::SpamTrigger;
use super::{tx_actor::TxActorHandle, OnTxSent};

#[derive(Clone)]
pub struct SpamRunContext {
    done_sending: Arc<AtomicBool>,
    do_quit: tokio_util::sync::CancellationToken,
}

impl SpamRunContext {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for SpamRunContext {
    fn default() -> Self {
        Self {
            done_sending: Arc::new(AtomicBool::new(false)),
            do_quit: tokio_util::sync::CancellationToken::new(),
        }
    }
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

    fn context(&self) -> &SpamRunContext;

    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>;

    fn spam_rpc(
        &self,
        scenario: &mut TestScenario<D, S, P>,
        txs_per_period: u64,
        num_periods: u64,
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
            let mut cursor = self.on_spam(scenario).await?.take(num_periods as usize);

            // calling cancel() on cancel_token should stop all running tasks
            // (as long as each task checks for it)
            let cancel_token = self.context().do_quit.clone();

            // run spammer within tokio::select! to allow for graceful shutdown
            let spam_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("\nCTRL-C received, stopping spamming...");
                    cancel_token.cancel();

                    false
                },
                _ = scenario.execute_spammer(&mut cursor, &tx_req_chunks, sent_tx_callback) => {
                    true
                }
            };
            if !spam_finished {
                info!("Spammer terminated. Press CTRL-C again to stop result collection...");
            }
            self.context()
                .done_sending
                .store(true, std::sync::atomic::Ordering::SeqCst);

            // collect results from cached pending txs
            let flush_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("\nCTRL-C received, stopping result collection...");
                    let _ = scenario.msg_handle.stop().await;
                    cancel_token.cancel();
                    false
                },
                _ = scenario.flush_tx_cache(start_block, run_id.unwrap_or(0)) => {
                    true
                }
            };
            if !flush_finished {
                info!("Result collection terminated. Some pending txs may not have been saved to the database.");
            }

            // clear out unconfirmed txs from the cache
            let dump_finished: bool = tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("\nCTRL-C received, stopping tx cache dump...");
                    cancel_token.cancel();
                    false
                },
                _ = scenario.dump_tx_cache(run_id.unwrap_or(0)) => {
                    true
                }
            };
            if !dump_finished {
                info!("Tx cache dump terminated. Some unconfirmed txs may not have been saved to the database.");
            }

            if let Some(run_id) = run_id {
                let latency_metrics = scenario.collect_latency_metrics();
                scenario
                    .db
                    .insert_latency_metrics(run_id, &latency_metrics)?;
            }

            info!(
                "done. {}",
                run_id.map(|id| format!("run_id: {id}")).unwrap_or_default()
            );
            Ok(())
        }
    }
}
