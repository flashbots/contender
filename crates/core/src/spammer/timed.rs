use std::time::Duration;
use std::{pin::Pin, sync::Arc};

use futures::Stream;
use futures::StreamExt;

use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
};

use super::{OnTxSent, SpamTrigger, Spammer};

pub struct TimedSpammer<F>
where
    F: OnTxSent + Send + Sync + 'static,
{
    callback_handle: Arc<F>,
    wait_interval: Duration,
}

impl<F> TimedSpammer<F>
where
    F: OnTxSent + Send + Sync + 'static,
{
    pub fn new<D: DbOps + Send + Sync + 'static>(
        callback_handle: F,
        wait_interval: Duration,
    ) -> Self {
        Self {
            callback_handle: Arc::new(callback_handle),
            wait_interval,
        }
    }
}

impl<F, D, S, P> Spammer<F, D, S, P> for TimedSpammer<F>
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
{
    fn sent_tx_callback(&self) -> std::sync::Arc<F> {
        self.callback_handle.clone()
    }

    fn on_spam(
        &self,
        _scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = crate::Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>
    {
        let interval = self.wait_interval;
        async move {
            let do_poll = move |tick| async move {
                tokio::time::sleep(interval).await;
                tick
            };
            Ok(
                futures::stream::unfold(0, move |t| async move { Some((do_poll(t).await, t + 1)) })
                    .map(SpamTrigger::Tick)
                    .boxed(),
            )
        }
    }
}
