use std::sync::atomic::AtomicBool;
use std::time::Duration;
use std::{pin::Pin, sync::Arc};

use alloy::providers::Provider;
use contender_engine_provider::DEFAULT_BLOCK_TIME;
use futures::Stream;
use futures::StreamExt;
use tracing::{debug, info, warn};

use super::tx_callback::OnBatchSent;
use super::OnTxSent;
use super::SpamTrigger;
use crate::db::SpamDuration;
use crate::spammer::tx_actor::ActorContext;
use crate::spammer::CallbackError;
use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
    Result,
};

#[derive(Clone)]
pub struct SpamRunContext {
    done_sending: Arc<AtomicBool>,
    done_fcu: Arc<AtomicBool>,
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
            done_fcu: Arc::new(AtomicBool::new(false)),
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
    fn context(&self) -> &SpamRunContext;

    fn on_spam(
        &self,
        scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>;

    fn duration_units(periods: u64) -> SpamDuration;

    fn spam_rpc(
        &self,
        scenario: &mut TestScenario<D, S, P>,
        txs_per_period: u64,
        num_periods: u64,
        run_id: Option<u64>,
        sent_tx_callback: Arc<F>,
    ) -> impl std::future::Future<Output = crate::Result<()>> {
        async move {
            let run_id = run_id.unwrap_or(scenario.db.num_runs().map_err(|e| e.into())?);
            let is_fcu_done = self.context().done_fcu.clone();
            let is_sending_done = self.context().done_sending.clone();
            let auth_provider = scenario.auth_provider.clone();
            let start_block = scenario.rpc_client.get_block_number().await.map_err(|e| {
                CallbackError::ProviderCall(format!("failed to get block number: {e}"))
            })?;

            // run loop in background to call fcu when spamming is done
            let fcu_handle: tokio::task::JoinHandle<Result<()>> = tokio::task::spawn(async move {
                if let Some(auth_client) = &auth_provider {
                    loop {
                        let fcu_done = is_fcu_done.load(std::sync::atomic::Ordering::SeqCst);
                        let sending_done =
                            is_sending_done.load(std::sync::atomic::Ordering::SeqCst);
                        if fcu_done {
                            info!("FCU is done, stopping block production...");
                            break;
                        }
                        if sending_done {
                            auth_client
                                .advance_chain(DEFAULT_BLOCK_TIME)
                                .await
                                .map_err(|e| {
                                    is_fcu_done.store(true, std::sync::atomic::Ordering::SeqCst);
                                    CallbackError::AuthProvider(e)
                                })?;
                        } else {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                Ok(())
            });

            let tx_req_chunks = scenario
                .get_spam_tx_chunks(txs_per_period, num_periods)
                .await?;
            let mut cursor = self.on_spam(scenario).await?.take(num_periods as usize);

            if scenario.should_sync_nonces {
                scenario.sync_nonces().await?;
            }

            let actor_ctx = ActorContext::new(start_block, run_id);
            scenario.tx_actor().init_ctx(actor_ctx).await?;

            let actor_ctx = ActorContext::new(start_block, run_id);
            scenario.tx_actor().init_ctx(actor_ctx).await?;

            // run spammer within tokio::select! to allow for graceful shutdown
            let do_quit = scenario.ctx.cancel_token.clone();
            let spam_finished: bool = tokio::select! {
                _ = do_quit.cancelled() => {
                    debug!("CTRL-C received, dropping execute_spammer call...");


                    false
                },
                res = scenario.execute_spammer(&mut cursor, &tx_req_chunks, sent_tx_callback) => {
                    if res.as_ref().is_err() {
                        return res;
                    }
                    true
                }
            };
            if !spam_finished {
                warn!("Spammer terminated. Press CTRL-C again to stop result collection...");
            }
            self.context()
                .done_sending
                .store(true, std::sync::atomic::Ordering::SeqCst);

            fcu_handle.await.map_err(CallbackError::Join)??;

            // clear out unconfirmed txs from the cache
            let dump_finished: bool = tokio::select! {
                _ = scenario.ctx.cancel_token.cancelled() => {
                    warn!("CTRL-C received, stopping tx cache dump...");
                    scenario.tx_actor().stop().await?;
                    false
                },
                _ = scenario.dump_tx_cache(run_id) => {
                    true
                }
            };
            if !dump_finished {
                warn!("Tx cache dump terminated. Some unconfirmed txs may not have been saved to the database.");
            }

            self.context()
                .done_fcu
                .store(true, std::sync::atomic::Ordering::SeqCst);

            let latency_metrics = scenario.collect_latency_metrics();
            scenario
                .db
                .insert_latency_metrics(run_id, &latency_metrics)
                .map_err(|e| e.into())?;

            info!("done. run_id: {run_id}");
            Ok(())
        }
    }
}
