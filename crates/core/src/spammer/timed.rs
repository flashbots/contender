use std::pin::Pin;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;

use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
};

use super::spammer_trait::SpamRunContext;
use super::tx_callback::OnBatchSent;
use super::{OnTxSent, SpamTrigger, Spammer};

pub struct TimedSpammer {
    wait_interval: Duration,
    context: SpamRunContext,
}

impl TimedSpammer {
    pub fn new(wait_interval: Duration) -> Self {
        Self {
            wait_interval,
            context: SpamRunContext::new(),
        }
    }
}

impl<F, D, S, P> Spammer<F, D, S, P> for TimedSpammer
where
    F: OnTxSent + OnBatchSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
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

    fn context(&self) -> &SpamRunContext {
        &self.context
    }
}
