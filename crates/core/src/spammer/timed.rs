use std::pin::Pin;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;

use crate::{
    db::DbOps,
    generator::{seeder::Seeder, templater::Templater, PlanConfig},
    test_scenario::TestScenario,
};

use super::{OnTxSent, SpamTrigger, Spammer};

pub struct TimedSpammer {
    wait_interval: Duration,
}

impl TimedSpammer {
    pub fn new(wait_interval: Duration) -> Self {
        Self { wait_interval }
    }
}

impl<F, D, S, P> Spammer<F, D, S, P> for TimedSpammer
where
    F: OnTxSent + Send + Sync + 'static,
    D: DbOps + Send + Sync + 'static,
    S: Seeder + Send + Sync,
    P: PlanConfig<String> + Templater<String> + Send + Sync,
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
                    .map(|t| SpamTrigger::Tick(t))
                    .boxed(),
            )
        }
    }
}
