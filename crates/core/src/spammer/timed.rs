use std::pin::Pin;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;
use tokio::time::{interval, MissedTickBehavior};

use crate::generator::seeder::rand_seed::SeedGenerator;
use crate::{
    db::DbOps,
    generator::{templater::Templater, PlanConfig},
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
    S: SeedGenerator + Send + Sync + Clone,
    P: PlanConfig<String> + Templater<String> + Send + Sync + Clone,
{
    fn on_spam(
        &self,
        _scenario: &mut TestScenario<D, S, P>,
    ) -> impl std::future::Future<Output = crate::Result<Pin<Box<dyn Stream<Item = SpamTrigger> + Send>>>>
    {
        let wait_interval = self.wait_interval;
        async move {
            // Use tokio::time::interval for consistent timing that doesn't drift
            // even when batch processing takes variable time
            let mut tick_interval = interval(wait_interval);
            // Skip the first immediate tick - we want to wait before the first batch
            tick_interval.tick().await;
            // If processing takes longer than interval, delay the next tick
            // rather than bursting. Burst causes cascading delays because queued
            // ticks fire immediately, giving deferred task collections no
            // background processing time.
            tick_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            Ok(
                futures::stream::unfold((0u64, tick_interval), |(tick, mut interval)| async move {
                    interval.tick().await;
                    Some((SpamTrigger::Tick(tick), (tick + 1, interval)))
                })
                .boxed(),
            )
        }
    }

    fn duration_units(periods: u64) -> crate::db::SpamDuration {
        crate::db::SpamDuration::Seconds(periods)
    }

    fn context(&self) -> &SpamRunContext {
        &self.context
    }
}
